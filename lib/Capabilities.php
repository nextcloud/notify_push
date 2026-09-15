<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush;

use OCP\Capabilities\IPublicCapability;
use OCP\IConfig;
use OCP\IURLGenerator;
use OCP\IUserSession;

class Capabilities implements IPublicCapability {
	private $config;
	private $urlGenerator;
	private $userSession;

	public function __construct(IConfig $config, IURLGenerator $urlGenerator, IUserSession $userSession) {
		$this->config = $config;
		$this->urlGenerator = $urlGenerator;
		$this->userSession = $userSession;
	}

	public function getCapabilities() {
		$baseEndpoint = $this->config->getAppValue('notify_push', 'base_endpoint');

		if (!$baseEndpoint) {
			return [];
		}

		$wsEndpoint = str_replace('https://', 'wss://', $baseEndpoint);
		$wsEndpoint = str_replace('http://', 'ws://', $wsEndpoint) . '/ws';

		// endpoints that are usable without a user session
		$capabilities = [
			'notify_push' => [
				'endpoints' => [
					'websocket' => $wsEndpoint,
					'anon_pre_auth' => $this->url('notify_push.AnonymousAuth.preAuth'),
				],
			],
		];

		if ($this->userSession->isLoggedIn()) {
			$capabilities['notify_push']['type'] = ['files', 'activities', 'notifications'];
			$capabilities['notify_push']['endpoints']['pre_auth'] = $this->url('notify_push.Auth.preAuth');
		}

		return $capabilities;
	}

	private function url(string $route): string {
		return $this->urlGenerator->getAbsoluteURL($this->urlGenerator->linkToRoute($route));
	}
}
