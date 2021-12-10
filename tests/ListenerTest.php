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
use OCP\Group\Events\UserAddedEvent;
use OCP\Group\Events\UserRemovedEvent;
use OCP\IGroup;
use OCP\IUser;
use OCP\Share\Events\ShareCreatedEvent;
use OCP\Share\IShare;
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

		// file ids are unstable, so we remove them
		foreach ($events['notify_storage_update'] as &$event) {
			unset($event['file_id']);
		}

		$this->assertEquals([
			'notify_storage_update' => [
				['storage' => 1, 'path' => 'foobar'],
			],
		], $events);
	}

	public function testGroupEvents() {
		$events = [];
		$queue = $this->getQueue($events);
		$listener = new Listener($queue);

		$user = $this->createMock(IUser::class);
		$user->method('getUID')->willReturn('user1');

		$group = $this->createMock(IGroup::class);
		$group->method('getGID')->willReturn('group1');

		$listener->groupListener(new UserAddedEvent(
			$group,
			$user
		));
		$this->assertEquals([
			'notify_group_membership_update' => [
				['user' => 'user1', 'group' => 'group1'],
			],
		], $events);

		$events = [];

		$listener->groupListener(new UserRemovedEvent(
			$group,
			$user
		));
		$this->assertEquals([
			'notify_group_membership_update' => [
				['user' => 'user1', 'group' => 'group1'],
			],
		], $events);
	}

	public function testShareEvents() {
		$events = [];
		$queue = $this->getQueue($events);
		$listener = new Listener($queue);

		$share = $this->createMock(IShare::class);
		$share->method('getShareType')
			->willReturn(IShare::TYPE_USER);
		$share->method('getSharedWith')
			->willReturn('user1');

		$listener->shareListener(new ShareCreatedEvent(
			$share
		));
		$this->assertEquals([
			'notify_user_share_created' => [
				['user' => 'user1'],
			],
		], $events);
	}
}
