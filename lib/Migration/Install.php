<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Migration;

use OCA\NotifyPush\AnonymousSessionManager;
use OCA\NotifyPush\AppInfo\Application;
use OCA\NotifyPush\BinaryFinder;
use OCP\IAppConfig;
use OCP\Migration\IOutput;
use OCP\Migration\IRepairStep;
use OCP\Security\ISecureRandom;

class Install implements IRepairStep {
	private $binaryFinder;
	private $appConfig;
	private $random;

	public function __construct(BinaryFinder $setupWizard, IAppConfig $appConfig, ISecureRandom $random) {
		$this->binaryFinder = $setupWizard;
		$this->appConfig = $appConfig;
		$this->random = $random;
	}

	public function getName() {
		return 'Set binary permissions';
	}

	/**
	 * @return void
	 */
	public function run(IOutput $output) {
		$path = $this->binaryFinder->getBinaryPath();
		@chmod($path, 0755);

		$this->setupAnonymousSessionKey();
	}

	/**
	 * Generate the key used to sign anonymous session tokens ahead of time, so concurrent
	 * requests can't race each other generating it.
	 */
	private function setupAnonymousSessionKey(): void {
		if ($this->appConfig->getValueString(Application::APP_ID, AnonymousSessionManager::SIGNING_KEY_CONFIG_KEY) !== '') {
			return;
		}

		$this->appConfig->setValueString(
			Application::APP_ID,
			AnonymousSessionManager::SIGNING_KEY_CONFIG_KEY,
			$this->random->generate(AnonymousSessionManager::SIGNING_KEY_LENGTH, ISecureRandom::CHAR_ALPHANUMERIC),
			sensitive: true,
		);
	}
}
