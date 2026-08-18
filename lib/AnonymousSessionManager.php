<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2026 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush;

use OCA\NotifyPush\AppInfo\Application;
use OCA\NotifyPush\Queue\IQueue;
use OCP\AppFramework\Utility\ITimeFactory;
use OCP\IAppConfig;
use OCP\Security\ISecureRandom;

/**
 * Anonymous sessions are not stored server side, instead the session id is handed to the client
 * as part of a token that is signed with an instance wide key.
 *
 * This keeps the session usable across reconnects without having to keep state for clients
 * that might never connect in the first place.
 */
class AnonymousSessionManager implements IAnonymousSessionManager {
	public const SIGNING_KEY_CONFIG_KEY = 'anonymous_session_key';
	public const SIGNING_KEY_LENGTH = 64;

	private const SESSION_ID_LENGTH = 32;
	private const PRE_AUTH_TOKEN_LENGTH = 32;
	private const APP_ID_PATTERN = '/^[a-z0-9_.-]+$/';
	private const SESSION_ID_PATTERN = '/^[a-z0-9_.-]+:[a-zA-Z0-9]+$/';

	public function __construct(
		private readonly IQueue $queue,
		private readonly ISecureRandom $random,
		private readonly IAppConfig $appConfig,
		private readonly ITimeFactory $timeFactory,
	) {
	}

	#[\Override]
	public function createSession(string $appId, int $ttl = self::DEFAULT_TTL): AnonymousSession {
		if ($appId === '' || preg_match(self::APP_ID_PATTERN, $appId) !== 1) {
			throw new \InvalidArgumentException('Invalid app id: ' . $appId);
		}

		$id = $appId . ':' . $this->random->generate(self::SESSION_ID_LENGTH, ISecureRandom::CHAR_ALPHANUMERIC);

		return $this->renewToken($id, $ttl);
	}

	#[\Override]
	public function renewToken(string $sessionId, int $ttl = self::DEFAULT_TTL): AnonymousSession {
		if (preg_match(self::SESSION_ID_PATTERN, $sessionId) !== 1) {
			throw new \InvalidArgumentException('Invalid session id: ' . $sessionId);
		}
		if ($ttl <= 0 || $ttl > self::MAX_TTL) {
			throw new \InvalidArgumentException('Session ttl needs to be between 1 and ' . self::MAX_TTL . ' seconds');
		}

		$expiration = $this->timeFactory->getTime() + $ttl;

		return new AnonymousSession($sessionId, $this->createToken($sessionId, $expiration), $expiration);
	}

	#[\Override]
	public function send(string $sessionId, string $message, mixed $body = null): void {
		$this->queue->push('notify_custom', [
			'session' => $sessionId,
			'message' => $message,
			'body' => $body,
		]);
	}

	/**
	 * Get the id of the session a token belongs to, or null if the token is invalid or expired.
	 */
	public function validateToken(string $token): ?string {
		$parts = explode('.', $token);
		if (count($parts) !== 2) {
			return null;
		}
		[$payload, $signature] = $parts;

		if (!hash_equals($this->sign($payload), $signature)) {
			return null;
		}

		$decoded = json_decode($this->decode($payload), true);
		if (!is_array($decoded) || !isset($decoded['id'], $decoded['exp'])
			|| !is_string($decoded['id']) || !is_int($decoded['exp'])) {
			return null;
		}
		if ($decoded['exp'] <= $this->timeFactory->getTime()) {
			return null;
		}

		return $decoded['id'];
	}

	/**
	 * Announce a session to the push server and get a short lived token the client can use
	 * to authenticate a websocket connection.
	 */
	public function preAuthenticate(string $sessionId): string {
		$token = $this->random->generate(self::PRE_AUTH_TOKEN_LENGTH);

		$this->queue->push('notify_pre_auth', [
			'session' => $sessionId,
			'token' => $token,
		]);

		return $token;
	}

	private function createToken(string $id, int $expiration): string {
		$payload = $this->encode(json_encode([
			'id' => $id,
			'exp' => $expiration,
		], JSON_THROW_ON_ERROR));

		return $payload . '.' . $this->sign($payload);
	}

	private function sign(string $payload): string {
		return $this->encode(hash_hmac('sha256', $payload, $this->getSigningKey(), true));
	}

	private function getSigningKey(): string {
		$key = $this->appConfig->getValueString(Application::APP_ID, self::SIGNING_KEY_CONFIG_KEY);
		if ($key === '') {
			$key = $this->random->generate(self::SIGNING_KEY_LENGTH, ISecureRandom::CHAR_ALPHANUMERIC);
			$this->appConfig->setValueString(Application::APP_ID, self::SIGNING_KEY_CONFIG_KEY, $key, sensitive: true);
		}
		return $key;
	}

	private function encode(string $data): string {
		return rtrim(strtr(base64_encode($data), '+/', '-_'), '=');
	}

	private function decode(string $data): string {
		return (string)base64_decode(strtr($data, '-_', '+/'), true);
	}
}
