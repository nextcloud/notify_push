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

namespace OCA\NotifyPush\Controller;

use OCA\NotifyPush\Queue\IQueue;
use OCP\AppFramework\Controller;
use OCP\AppFramework\Http\DataDisplayResponse;
use OCP\IRequest;
use OCP\IUserSession;
use OCP\Security\ISecureRandom;

class PreAuthController extends Controller {
	private $queue;
	private $random;
	private $userSession;

	public function __construct(
		IRequest $request,
		IQueue $queue,
		ISecureRandom $random,
		IUserSession $userSession
	) {
		parent::__construct('notify_push', $request);
		$this->queue = $queue;
		$this->random = $random;
		$this->userSession = $userSession;
	}

	/**
	 * @NoAdminRequired
	 * @return DataDisplayResponse
	 */
	public function preAuth() {
		$token = $this->random->generate(32);

		$this->queue->push('notify_pre_auth', [
			'user' => $this->userSession->getUser()->getUID(),
			'token' => $token,
		]);

		return new DataDisplayResponse($token);
	}
}
