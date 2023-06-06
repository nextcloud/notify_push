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
		if (strpos($arch, 'amd64') == 0 && strpos($os, 'FreBSD') == 0) {
			return 'fbsd_amd64';
		}
		return $arch;
	}

	public function getBinaryPath(): string {
		$basePath = realpath(__DIR__ . '/../bin/');
		$arch = $this->getArch();
		return "$basePath/$arch/notify_push";
	}
}
