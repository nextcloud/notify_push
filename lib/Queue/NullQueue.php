<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Queue;

class NullQueue implements IQueue {
	/**
	 * @return void
	 */
	public function push(string $channel, $message) {
		// noop
	}
}
