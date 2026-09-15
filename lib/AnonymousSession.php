<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2026 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush;

/**
 * A push session for a client without a user session.
 *
 * @see IAnonymousSessionManager::createSession()
 */
class AnonymousSession {
	public function __construct(
		private readonly string $id,
		private readonly string $token,
		private readonly int $expiration,
	) {
	}

	/**
	 * The id of the session, keep this server side to send messages to the session.
	 *
	 * @see IAnonymousSessionManager::send()
	 */
	public function getId(): string {
		return $this->id;
	}

	/**
	 * The token the client needs to connect to the push server, hand this to the client.
	 *
	 * The token is a secret, anyone holding it can receive the messages sent to this session.
	 */
	public function getToken(): string {
		return $this->token;
	}

	/**
	 * Unix timestamp after which the token stops working and the client can no longer (re)connect
	 */
	public function getExpiration(): int {
		return $this->expiration;
	}
}
