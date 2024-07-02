<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush;

use OCP\Capabilities\ICapability;
use OCP\IConfig;
use OCP\IURLGenerator;

class Capabilities implements ICapability {
	private $config;
	private $urlGenerator;

	public function __construct(IConfig $config, IURLGenerator $urlGenerator) {
		$this->config = $config;
		$this->urlGenerator = $urlGenerator;
	}

	public function getCapabilities() {
		$baseEndpoint = $this->config->getAppValue('notify_push', 'base_endpoint');

		$wsEndpoint = str_replace('https://', 'wss://', $baseEndpoint);
		$wsEndpoint = str_replace('http://', 'ws://', $wsEndpoint) . '/ws';

		if ($baseEndpoint) {
			return [
				'notify_push' => [
					'type' => ['files', 'activities', 'notifications'],
					'endpoints' => [
						'websocket' => $wsEndpoint,
						'pre_auth' => $this->urlGenerator->getAbsoluteURL($this->urlGenerator->linkToRoute('notify_push.Auth.preAuth'))
					],
				],
			];
		} else {
			return [];
		}
	}
}
