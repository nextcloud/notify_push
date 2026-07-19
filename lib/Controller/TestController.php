<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Controller;

use OC\AppFramework\Http\Request;
use OCA\NotifyPush\Queue\IQueue;
use OCP\App\IAppManager;
use OCP\AppFramework\Controller;
use OCP\AppFramework\Http;
use OCP\AppFramework\Http\DataDisplayResponse;
use OCP\AppFramework\Http\DataResponse;
use OCP\AppFramework\Http\Response;
use OCP\IAppConfig;
use OCP\IRequest;

class TestController extends Controller {
	public function __construct(
		IRequest $request,
		private IAppConfig $appConfig,
		private IQueue $queue,
		private IAppManager $appManager,
	) {
		parent::__construct('notify_push', $request);
	}

	/**
	 * @NoAdminRequired
	 * @PublicPage
	 * @NoCSRFRequired
	 */
	public function cookie(): Response {
		// starting with 32, the app config does some internal caching
		// that interferes with the quick set+get from this test.
		$this->appConfig->clearCache();

		$expected = $this->queue->get('test-token');
		$token = $this->request->getHeader('token');
		if ($expected !== $token) {
			return new Response(Http::STATUS_FORBIDDEN);
		}

		return new DataResponse($this->appConfig->getValueInt('notify_push', 'cookie'));
	}

	/**
	 * @NoAdminRequired
	 * @PublicPage
	 * @NoCSRFRequired
	 */
	public function remote(): Response {
		$expected = $this->queue->get('test-token');
		$token = $this->request->getHeader('token');
		if ($expected !== $token) {
			return new Response(Http::STATUS_FORBIDDEN);
		}

		if ($this->request instanceof Request) {
			$this->queue->set('notify_push_forwarded_header', $this->request->getHeader('x-forwarded-for'));
			$this->queue->set('notify_push_remote', $this->request->server['REMOTE_ADDR']);
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
		$this->queue->set('notify_push_app_version', $this->appManager->getAppVersion('notify_push'));
	}
}
