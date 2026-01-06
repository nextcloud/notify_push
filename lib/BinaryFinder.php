<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush;

class BinaryFinder {
	public function getArch(): string {
		$arch = php_uname('m');
		$os = php_uname('s');
		if (strpos($arch, 'armv7') === 0) {
			return 'armv7';
		}
		if (strpos($arch, 'aarch64') === 0) {
			return 'aarch64';
		}
		if (strpos($os, 'FreeBSD') === 0) {
			$arch = 'fbsd_' . $arch;
		}
		return $arch;
	}

	public function getBinaryPath(): string {
		$basePath = realpath(__DIR__ . '/../bin/');
		$arch = $this->getArch();
		return "$basePath/$arch/notify_push";
	}
}
