<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
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
			$this->logger->warning('Malformed push server configured: ' . $baseEndpoint);
			return;
		}

		$connect = $endPointUrl['host'];
		if (isset($endPointUrl['port'])) {
			$connect .= ':' . $endPointUrl['port'];
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
