<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2026 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Tests;

use OCA\NotifyPush\BinaryFinder;
use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\SelfTest;
use OCA\NotifyPush\SetupWizard;
use OCP\Http\Client\IClient;
use OCP\Http\Client\IClientService;
use OCP\IConfig;
use Test\TestCase;

class SetupWizardBaseUrlTest extends TestCase {
	/** @var string[] every url the wizard actually requested */
	private array $requested = [];

	private function wizard(string $cliUrl, bool $httpsReachable): SetupWizard {
		$this->requested = [];

		$config = $this->createMock(IConfig::class);
		$config->method('getSystemValueString')
			->willReturnCallback(function (string $key, string $default = '') use ($cliUrl) {
				return $key === 'overwrite.cli.url' ? $cliUrl : $default;
			});

		$client = $this->createMock(IClient::class);
		$client->method('get')->willReturnCallback(function (string $url) use ($httpsReachable) {
			$this->requested[] = $url;
			if (!$httpsReachable) {
				throw new \Exception('connection refused');
			}
			return $this->createMock(\OCP\Http\Client\IResponse::class);
		});

		$clientService = $this->createMock(IClientService::class);
		$clientService->method('newClient')->willReturn($client);

		return new SetupWizard(
			$this->createMock(IQueue::class),
			$this->createMock(SelfTest::class),
			$clientService,
			$config,
			$this->createMock(BinaryFinder::class),
		);
	}

	public function testProbesTheHttpsUrlNotTheHttpOne(): void {
		$wizard = $this->wizard('http://push.example.com', true);
		$wizard->getProxiedBase();

		$this->assertEquals(
			['https://push.example.com'],
			$this->requested,
			'the wizard has to probe the https url it built, not the http one it started from'
		);
	}

	public function testUsesHttpsWhenReachable(): void {
		$wizard = $this->wizard('http://push.example.com', true);
		$this->assertEquals('https://push.example.com/push', $wizard->getProxiedBase());
	}

	public function testFallsBackToHttpWhenHttpsIsNotReachable(): void {
		$wizard = $this->wizard('http://push.example.com', false);
		$this->assertEquals('http://push.example.com/push', $wizard->getProxiedBase());
	}

	public function testAnswersTheSameOnEveryCall(): void {
		$wizard = $this->wizard('http://push.example.com', true);

		$first = $wizard->getProxiedBase();
		$second = $wizard->getProxiedBase();

		$this->assertEquals($first, $second, 'the base url changed between calls');
		$this->assertCount(1, $this->requested, 'the https probe was not cached');
	}

	public function testHttpsUrlIsLeftAlone(): void {
		$wizard = $this->wizard('https://push.example.com', true);
		$this->assertEquals('https://push.example.com/push', $wizard->getProxiedBase());
		$this->assertCount(0, $this->requested, 'no probe is needed for a url that is already https');
	}

	public function testEmptyUrlIsNotProbed(): void {
		$wizard = $this->wizard('', true);
		$this->assertEquals('/push', $wizard->getProxiedBase());
		$this->assertCount(0, $this->requested);
	}
}
