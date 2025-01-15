<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Controller;

use OCA\NotifyPush\Queue\IQueue;
use OCP\AppFramework\Controller;
use OCP\AppFramework\Http\DataDisplayResponse;
use OCP\IRequest;
use OCP\IUserSession;
use OCP\Security\ISecureRandom;

class AuthController extends Controller {
	private $queue;
	private $random;
	private $userSession;

	public function __construct(
		IRequest $request,
		IQueue $queue,
		ISecureRandom $random,
		IUserSession $userSession,
	) {
		parent::__construct('notify_push', $request);
		$this->queue = $queue;
		$this->random = $random;
		$this->userSession = $userSession;
	}

	/**
	 * @NoAdminRequired
	 * @NoCSRFRequired
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

	/**
	 * @NoAdminRequired
	 * @NoCSRFRequired
	 * @return DataDisplayResponse
	 */
	public function getUid() {
		return new DataDisplayResponse($this->userSession->getUser()->getUID());
	}
}
