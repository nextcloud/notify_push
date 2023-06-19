<?php

declare(strict_types=1);
/**
 * @copyright Copyright (c) 2021 Robin Appelman <robin@icewind.nl>
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

use OCP\AppFramework\Http\ContentSecurityPolicy;
use OCP\EventDispatcher\Event;
use OCP\EventDispatcher\IEventListener;
use OCP\IConfig;
use OCP\Security\CSP\AddContentSecurityPolicyEvent;
use Psr\Log\LoggerInterface;

/**
 * @implements IEventListener<AddContentSecurityPolicyEvent>
 */
class CSPListener implements IEventListener {
	private $config;
	private $logger;

	public function __construct(IConfig $config, LoggerInterface $logger) {
		$this->config = $config;
		$this->logger = $logger;
	}

	public function handle(Event $event): void {
		if (!($event instanceof AddContentSecurityPolicyEvent)) {
			return;
		}

		$csp = new ContentSecurityPolicy();

		$baseEndpoint = $this->config->getAppValue('notify_push', 'base_endpoint');
		if (!$baseEndpoint) {
			return;
		}
		$endPointUrl = parse_url($baseEndpoint);

		if (!isset($endPointUrl['host'])) {
			$this->logger->warning("Malformed push server configured: " . $baseEndpoint);
			return;
		}

		$connect = $endPointUrl['host'];
		if (isset($endPointUrl['port'])) {
			$connect .= ':'. $endPointUrl['port'];
		}
		if (isset($endPointUrl['scheme']) && $endPointUrl['scheme'] === 'https') {
			$connect = 'wss://' . $connect;
		} else {
			$connect = 'ws://' . $connect;
		}
		$csp->addAllowedConnectDomain($connect);

		$event->addPolicy($csp);
	}
}
