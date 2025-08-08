<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2025 Robin Appelman <robin@icewind.nl>
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Queue;

use OC\RedisFactory;
use OCP\IConfig;

class PushRedisFactory {
	private $instance = null;
	private IConfig $config;
	private RedisFactory $redisFactory;

	public function __construct(
		IConfig $config,
		RedisFactory $redisFactory,
	) {
		$this->config = $config;
		$this->redisFactory = $redisFactory;
	}

	/**
	 * @return \Redis|\RedisCluster|null
	 * @throws \Exception
	 */
	public function getRedis() {
		if ($this->instance) {
			return $this->instance;
		}
		if ($this->config->getSystemValue('notify_push_redis', []) !== []) {
			return $this->getSeparateRedis();
		} elseif ($this->redisFactory->isAvailable()) {
			return $this->redisFactory->getInstance();
		} else {
			return null;
		}
	}

	private function getSeparateRedis(): \Redis {
		$config = $this->config->getSystemValue('notify_push_redis', []);
		$timeout = $config['timeout'] ?? 0.0;
		$readTimeout = $config['read_timeout'] ?? 0.0;

		$auth = null;
		if (isset($config['password']) && (string)$config['password'] !== '') {
			if (isset($config['user']) && (string)$config['user'] !== '') {
				$auth = [$config['user'], $config['password']];
			} else {
				$auth = $config['password'];
			}
		}

		$persistent = $this->config->getSystemValue('notify_push_redis.persistent', true);

		$redis = new \Redis();

		$host = $config['host'] ?? '127.0.0.1';
		$port = $config['port'] ?? ($host[0] !== '/' ? 6379 : null);

		// Support for older phpredis versions not supporting connectionParameters
		if (isset($config['ssl_context'])) {
			// Non-clustered redis requires connection parameters to be wrapped inside `stream`
			$connectionParameters = [
				'stream' => $config['ssl_context']
			];
			if ($persistent) {
				/**
				 * even though the stubs and documentation don't want you to know this,
				 * pconnect does have the same $connectionParameters argument connect has
				 *
				 * https://github.com/phpredis/phpredis/blob/0264de1824b03fb2d0ad515b4d4ec019cd2dae70/redis.c#L710-L730
				 *
				 * @psalm-suppress TooManyArguments
				 */
				$redis->pconnect($host, $port, $timeout, null, 0, $readTimeout, $connectionParameters);
			} else {
				$redis->connect($host, $port, $timeout, null, 0, $readTimeout, $connectionParameters);
			}
		} else {
			if ($persistent) {
				$redis->pconnect($host, $port, $timeout, null, 0, $readTimeout);
			} else {
				$redis->connect($host, $port, $timeout, null, 0, $readTimeout);
			}
		}

		if ($auth !== null) {
			$redis->auth($auth);
		}

		if (isset($config['dbindex'])) {
			$redis->select($config['dbindex']);
		}

		return $redis;
	}
}
