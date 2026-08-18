<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2026 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Tests;

use OCA\NotifyPush\AnonymousSessionManager;
use OCA\NotifyPush\IAnonymousSessionManager;
use OCA\NotifyPush\Queue\IQueue;
use OCP\AppFramework\Utility\ITimeFactory;
use OCP\IAppConfig;
use OCP\Security\ISecureRandom;
use Test\TestCase;

class AnonymousSessionManagerTest extends TestCase {
	private array $events = [];
	private int $time = 1000;
	private string $signingKey = '';
	private int $randomCounter = 0;

	private function getManager(): AnonymousSessionManager {
		$queue = $this->createMock(IQueue::class);
		$queue->method('push')->willReturnCallback(function ($channel, $event) {
			$this->events[$channel][] = $event;
		});

		$random = $this->createMock(ISecureRandom::class);
		$random->method('generate')->willReturnCallback(function (int $length) {
			$this->randomCounter++;
			return str_pad('r' . $this->randomCounter, $length, 'x');
		});

		$appConfig = $this->createMock(IAppConfig::class);
		$appConfig->method('getValueString')->willReturnCallback(fn () => $this->signingKey);
		$appConfig->method('setValueString')->willReturnCallback(function ($app, $key, $value) {
			$this->signingKey = $value;
			return true;
		});

		$timeFactory = $this->createMock(ITimeFactory::class);
		$timeFactory->method('getTime')->willReturnCallback(fn () => $this->time);

		return new AnonymousSessionManager($queue, $random, $appConfig, $timeFactory);
	}

	public function testCreateAndValidate(): void {
		$manager = $this->getManager();

		$session = $manager->createSession('myapp');

		$this->assertStringStartsWith('myapp:', $session->getId());
		$this->assertEquals($this->time + IAnonymousSessionManager::DEFAULT_TTL, $session->getExpiration());
		$this->assertEquals($session->getId(), $manager->validateToken($session->getToken()));
	}

	public function testSessionIdsAreUnique(): void {
		$manager = $this->getManager();

		$this->assertNotEquals(
			$manager->createSession('myapp')->getId(),
			$manager->createSession('myapp')->getId(),
		);
	}

	public function testRenewToken(): void {
		$manager = $this->getManager();
		$session = $manager->createSession('myapp', 100);

		$this->time += 50;
		$renewed = $manager->renewToken($session->getId(), 100);

		$this->assertEquals($session->getId(), $renewed->getId());
		$this->assertNotEquals($session->getToken(), $renewed->getToken());
		$this->assertEquals($this->time + 100, $renewed->getExpiration());

		// the old token keeps working until it expires on its own
		$this->time += 51;
		$this->assertNull($manager->validateToken($session->getToken()));
		$this->assertEquals($session->getId(), $manager->validateToken($renewed->getToken()));
	}

	public function testRenewInvalidSessionId(): void {
		$this->expectException(\InvalidArgumentException::class);
		$this->getManager()->renewToken('not a session id');
	}

	public function testTokenStaysValidForReconnects(): void {
		$manager = $this->getManager();
		$session = $manager->createSession('myapp', 100);

		$this->time += 50;

		$this->assertEquals($session->getId(), $manager->validateToken($session->getToken()));
		$this->assertEquals($session->getId(), $manager->validateToken($session->getToken()));
	}

	public function testExpiredToken(): void {
		$manager = $this->getManager();
		$session = $manager->createSession('myapp', 100);

		$this->time += 101;

		$this->assertNull($manager->validateToken($session->getToken()));
	}

	/**
	 * @return array<string, array{string}>
	 */
	public static function invalidTokenProvider(): array {
		return [
			'empty' => [''],
			'no signature' => ['eyJpZCI6ICJteWFwcDpmb28iLCAiZXhwIjogOTk5OTk5OTk5OX0'],
			'too many parts' => ['a.b.c'],
			'garbage payload' => ['!!!.!!!'],
		];
	}

	/**
	 * @dataProvider invalidTokenProvider
	 */
	public function testInvalidToken(string $token): void {
		$this->assertNull($this->getManager()->validateToken($token));
	}

	public function testTamperedToken(): void {
		$manager = $this->getManager();
		$session = $manager->createSession('myapp', 100);

		[$payload, $signature] = explode('.', $session->getToken());
		$forgedPayload = rtrim(strtr(base64_encode(json_encode([
			'id' => 'otherapp:stolen',
			'exp' => $this->time + 100,
		])), '+/', '-_'), '=');

		$this->assertNull($manager->validateToken($forgedPayload . '.' . $signature));
		$this->assertNull($manager->validateToken($payload . '.' . strrev($signature)));
	}

	public function testTokenFromDifferentInstance(): void {
		$manager = $this->getManager();
		$session = $manager->createSession('myapp');

		// simulate a different instance, and therefore a different signing key
		$this->signingKey = '';
		$otherManager = $this->getManager();

		$this->assertNull($otherManager->validateToken($session->getToken()));
	}

	public function testInvalidAppId(): void {
		$this->expectException(\InvalidArgumentException::class);
		$this->getManager()->createSession('my app');
	}

	public function testInvalidTtl(): void {
		$this->expectException(\InvalidArgumentException::class);
		$this->getManager()->createSession('myapp', IAnonymousSessionManager::MAX_TTL + 1);
	}

	public function testSend(): void {
		$manager = $this->getManager();

		$manager->send('myapp:session1', 'my_message', ['foo' => 'bar']);
		$manager->send('myapp:session1', 'my_message');

		$this->assertEquals([
			[
				'session' => 'myapp:session1',
				'message' => 'my_message',
				'body' => ['foo' => 'bar'],
			],
			[
				'session' => 'myapp:session1',
				'message' => 'my_message',
				'body' => null,
			],
		], $this->events['notify_custom']);
	}

	public function testPreAuthenticate(): void {
		$manager = $this->getManager();

		$token = $manager->preAuthenticate('myapp:session1');

		$this->assertEquals([
			[
				'session' => 'myapp:session1',
				'token' => $token,
			],
		], $this->events['notify_pre_auth']);
	}
}
