<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Queue;

interface IQueue {
	/**
	 * @param string $channel
	 * @param mixed $message
	 * @return void
	 */
	public function push(string $channel, $message);
}
