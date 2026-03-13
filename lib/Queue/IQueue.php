<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Queue;

interface IQueue {
	/**
	 * @param mixed $message
	 */
	public function push(string $channel, $message): void;

	/**
	 * @param mixed $value
	 */
	public function set(string $key, $value): void;

	/**
	 * @return mixed
	 */
	public function get(string $key);
}
