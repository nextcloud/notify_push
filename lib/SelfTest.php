<?php

declare(strict_types=1);
/**
 * @copyright Copyright (c) 2020 Robin Appelman <robin@icewind.nl>
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

use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\RedisQueue;
use OCP\Http\Client\IClient;
use OCP\IConfig;
use OCP\IDBConnection;
use Symfony\Component\Console\Output\OutputInterface;

class SelfTest {
	private $client;
	private $server;
	private $cookie;
	private $config;
	private $queue;
	private $connection;

	public function __construct(
		IClient $client,
		IConfig $config,
		IQueue $queue,
		IDBConnection $connection,
		string $server
	) {
		$this->client = $client;
		$this->server = $server;
		$this->cookie = rand(1, pow(2, 30));
		$this->queue = $queue;
		$this->config = $config;
		$this->connection = $connection;
	}

	public function test(OutputInterface $output): int {
		if ($this->queue instanceof RedisQueue) {
			$output->writeln("<info>✓ redis is configured</info>");
		} else {
			$output->writeln("<error>🗴 redis is not configured</error>");
			return 1;
		}

		if (strpos($this->server, 'http://') === 0) {
			$output->writeln("<comment>🗴 using unencrypted https for push server is strongly discouraged</comment>");
		} elseif (strpos($this->server, 'https://') !== 0) {
			$output->writeln("<error>🗴 malformed server url</error>");
			return 1;
		}

		$this->queue->push('notify_test_cookie', $this->cookie);
		$this->config->setAppValue('notify_push', 'cookie', (string)$this->cookie);

		try {
			$retrievedCookie = (int)$this->client->get($this->server . '/test/cookie')->getBody();
		} catch (\Exception $e) {
			$msg = $e->getMessage();
			$output->writeln("<error>🗴 can't connect to push server: $msg</error>");
			return 1;
		}

		if ($this->cookie === $retrievedCookie) {
			$output->writeln("<info>✓ push server is receiving redis messages</info>");
		} else {
			$output->writeln("<error>🗴 push server is not receiving redis messages</error>");
			return 1;
		}

		// test if the push server can load storage mappings from the db
		[$storageId, $count] = $this->getStorageIdForTest();
		try {
			$retrievedCount = (int)$this->client->get($this->server . '/test/mapping/' . $storageId)->getBody();
		} catch (\Exception $e) {
			$msg = $e->getMessage();
			$output->writeln("<error>🗴 can't connect to push server: $msg</error>");
			return 1;
		}

		if ($count === $retrievedCount) {
			$output->writeln("<info>✓ push server can load mount info from database</info>");
		} else {
			$output->writeln("<error>🗴 push server can't load mount info from database</error>");
			return 1;
		}

		// test if the push server can reach nextcloud by having it request the cookie
		try {
			$retrievedCookie = (int)$this->client->get($this->server . '/test/reverse_cookie')->getBody();
		} catch (\Exception $e) {
			$msg = $e->getMessage();
			$output->writeln("<error>🗴 can't connect to push server: $msg</error>");
			return 1;
		}

		if ($this->cookie === $retrievedCookie) {
			$output->writeln("<info>✓ push server can connect to the Nextcloud server</info>");
		} else {
			$output->writeln("<error>🗴 push server can't connect to the Nextcloud server</error>");
			return 1;
		}

		// test that the push server is a trusted proxy
		try {
			$remote = $this->client->get($this->server . '/test/remote/1.2.3.4')->getBody();
		} catch (\Exception $e) {
			$msg = $e->getMessage();
			$output->writeln("<error>🗴 can't connect to push server: $msg</error>");
			return 1;
		}

		if ($remote === '1.2.3.4') {
			$output->writeln("<info>✓ push server is a trusted proxy</info>");
		} else {
			$output->writeln("<error>🗴 push server is not a trusted proxy, please add '$remote' to the list of trusted proxies" .
				" or configure any existing reverse proxy to forward the 'x-forwarded-for' send by the push server.</error>");
			return 1;
		}

		return 0;
	}

	private function getStorageIdForTest() {
		$query = $this->connection->getQueryBuilder();
		$query->select('storage_id', $query->func()->count())
			->from('mounts', 'm')
			->innerJoin('m', 'filecache', 'f', $query->expr()->eq('root_id', 'fileid'))
			->where($query->expr()->eq('path_hash', $query->createNamedParameter(md5(''))))
			->groupBy('storage_id')
			->setMaxResults(1);

		return $query->execute()->fetchNumeric();
	}
}
