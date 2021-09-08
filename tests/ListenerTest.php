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

namespace OCA\NotifyPush\Tests;

use OCA\NotifyPush\Listener;
use OCA\NotifyPush\Queue\IQueue;
use OCP\Files\Cache\CacheEntryInsertedEvent;
use OCP\Files\Storage\IStorage;
use Test\TestCase;

class ListenerTest extends TestCase {
	private function getQueue(array &$events) {
		$queue = $this->createMock(IQueue::class);
		$queue->method('push')->willReturnCallback(function ($channel, $event) use (&$events) {
			if (!isset($events[$channel])) {
				$events[$channel] = [];
			}
			$events[$channel][] = $event;
		});
		return $queue;
	}

	public function testCacheEvents() {
		$events = [];
		$queue = $this->getQueue($events);
		$listener = new Listener($queue);

		$listener->cacheListener(new CacheEntryInsertedEvent(
			$this->createMock(IStorage::class),
			'foobar',
			12,
			1
		));
		$this->assertEquals([
			'notify_storage_update' => [
				['storage' => 1, 'path' => 'foobar'],
			],
		], $events);
	}
}
