<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Migration;

use OCA\NotifyPush\BinaryFinder;
use OCP\Migration\IOutput;
use OCP\Migration\IRepairStep;

class Install implements IRepairStep {
	private $binaryFinder;

	public function __construct(BinaryFinder $setupWizard) {
		$this->binaryFinder = $setupWizard;
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
	}
}
