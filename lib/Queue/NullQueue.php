<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Queue;

class NullQueue implements IQueue {
	#[\Override]
	public function push(string $channel, $message): void {
		// noop
	}

	#[\Override]
	public function set(string $key, $value): void {
		// noop
	}

	#[\Override]
	public function get(string $key) {
		return null;
	}

}
