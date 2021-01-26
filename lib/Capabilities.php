<?php

declare(strict_types=1);
/**
 * @copyright Copyright (c) 2020 Robin Appelman <robin@icewind.nl>
 *
 * @license GNU AGPL version 3 or any later version
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 *
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
