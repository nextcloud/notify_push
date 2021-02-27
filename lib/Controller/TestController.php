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

namespace OCA\NotifyPush\Controller;

use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\RedisQueue;
use OCP\App\IAppManager;
use OCP\AppFramework\Controller;
use OCP\AppFramework\Http\DataDisplayResponse;
use OCP\AppFramework\Http\DataResponse;
use OCP\IConfig;
use OCP\IRequest;

class TestController extends Controller {
	private $config;
	private $queue;
	private $appManager;

	public function __construct(
		IRequest $request,
		IConfig $config,
		IQueue $queue,
		IAppManager $appManager
	) {
		parent::__construct('notify_push', $request);
		$this->config = $config;
		$this->queue = $queue;
		$this->appManager = $appManager;
	}

	/**
	 * @NoAdminRequired
	 * @PublicPage
	 * @NoCSRFRequired
	 */
	public function cookie(): DataResponse {
		return new DataResponse((int)$this->config->getAppValue('notify_push', 'cookie', '0'));
	}

	/**
	 * @NoAdminRequired
	 * @PublicPage
	 * @NoCSRFRequired
	 */
	public function remote(): DataDisplayResponse {
		if ($this->queue instanceof RedisQueue) {
			$this->queue->getConnection()->set("notify_push_forwarded_header", $this->request->getHeader('x-forwarded-for'));
			$this->queue->getConnection()->set("notify_push_remote", $this->request->server['REMOTE_ADDR']);
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
			$this->queue->getConnection()->set("notify_push_app_version", $this->appManager->getAppVersion('notify_push'));
		}
	}
}
