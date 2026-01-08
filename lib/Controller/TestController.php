<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Controller;

use OC\AppFramework\Http\Request;
use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\RedisQueue;
use OCP\App\IAppManager;
use OCP\AppFramework\Controller;
use OCP\AppFramework\Http\DataDisplayResponse;
use OCP\AppFramework\Http\DataResponse;
use OCP\IAppConfig;
use OCP\IConfig;
use OCP\IRequest;

class TestController extends Controller {
	private $config;
	private $appConfig;
	private $queue;
	private $appManager;

	public function __construct(
		IRequest $request,
		IConfig $config,
		IAppConfig $appConfig,
		IQueue $queue,
		IAppManager $appManager,
	) {
		parent::__construct('notify_push', $request);
		$this->config = $config;
		$this->appConfig = $appConfig;
		$this->queue = $queue;
		$this->appManager = $appManager;
	}

	/**
	 * @NoAdminRequired
	 * @PublicPage
	 * @NoCSRFRequired
	 */
	public function cookie(): DataResponse {
		// starting with 32, the app config does some internal caching
		// that interferes with the quick set+get from this test.
		// unfortunately the api to clear the cache is 29+ only, and we still support 27
		if (method_exists($this->appConfig, 'clearCache')) {
			/** @psalm-suppress UndefinedInterfaceMethod */
			$this->appConfig->clearCache();
		}
		return new DataResponse((int)$this->config->getAppValue('notify_push', 'cookie', '0'));
	}

	/**
	 * @NoAdminRequired
	 * @PublicPage
	 * @NoCSRFRequired
	 */
	public function remote(): DataDisplayResponse {
		if ($this->queue instanceof RedisQueue) {
			if ($this->request instanceof Request) {
				$this->queue->getConnection()->set('notify_push_forwarded_header', $this->request->getHeader('x-forwarded-for'));
				$this->queue->getConnection()->set('notify_push_remote', $this->request->server['REMOTE_ADDR']);
			}
		}
		return new DataDisplayResponse($this->request->getRemoteAddress());
	}

	/**
	 * @NoAdminRequired
	 *
	 * @PublicPage
	 *
	 * @NoCSRFRequired
	 *
	 * @return void
	 */
	public function version(): void {
		if ($this->queue instanceof RedisQueue) {
			$this->queue->getConnection()->set('notify_push_app_version', $this->appManager->getAppVersion('notify_push'));
		}
	}
}
