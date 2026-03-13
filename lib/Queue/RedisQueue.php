<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Queue;

class RedisQueue implements IQueue {
	private $redis;

	/**
	 * @param \Redis|\RedisCluster $redis
	 */
	public function __construct($redis) {
		$this->redis = $redis;
	}

	#[\Override]
	public function push(string $channel, $message): void {
		$this->redis->publish($channel, json_encode($message));
	}

	/**
	 * @return \Redis|\RedisCluster
	 */
	public function getConnection() {
		return $this->redis;
	}

	#[\Override]
	public function set(string $key, $value): void {
		$this->redis->set($key, $value);
	}

	#[\Override]
	public function get(string $key) {
		return $this->redis->get($key);
	}
}
