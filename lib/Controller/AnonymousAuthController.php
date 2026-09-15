<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2026 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Controller;

use OCA\NotifyPush\AnonymousSessionManager;
use OCA\NotifyPush\AppInfo\Application;
use OCP\AppFramework\Controller;
use OCP\AppFramework\Http;
use OCP\AppFramework\Http\Attribute\AnonRateLimit;
use OCP\AppFramework\Http\Attribute\NoCSRFRequired;
use OCP\AppFramework\Http\Attribute\PublicPage;
use OCP\AppFramework\Http\DataDisplayResponse;
use OCP\IRequest;

class AnonymousAuthController extends Controller {
	public function __construct(
		IRequest $request,
		private readonly AnonymousSessionManager $sessionManager,
	) {
		parent::__construct(Application::APP_ID, $request);
	}

	/**
	 * Exchange the token of an anonymous session for a short lived token that can be used
	 * to authenticate a websocket connection to the push server.
	 *
	 * The session token stays valid until it expires, so a client can call this again
	 * every time it needs to reconnect.
	 */
	#[PublicPage]
	#[NoCSRFRequired]
	#[AnonRateLimit(limit: 30, period: 60)]
	public function preAuth(string $token = ''): DataDisplayResponse {
		$sessionId = $this->sessionManager->validateToken($token);
		if ($sessionId === null) {
			return new DataDisplayResponse('Invalid or expired session token', Http::STATUS_UNAUTHORIZED);
		}

		return new DataDisplayResponse($this->sessionManager->preAuthenticate($sessionId));
	}
}
