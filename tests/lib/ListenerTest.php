<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Tests;

use OCA\NotifyPush\Listener;
use OCA\NotifyPush\Queue\IQueue;
use OCP\Files\Cache\CacheEntryInsertedEvent;
use OCP\Files\Cache\CacheEntryUpdatedEvent;
use OCP\Files\Cache\ICache;
use OCP\Files\Events\Node\NodeDeletedEvent;
use OCP\Files\File;
use OCP\Files\IHomeStorage;
use OCP\Files\NotFoundException;
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

	private function getHomeStorage(): IStorage {
		$cache = $this->createMock(ICache::class);
		$cache->method('getNumericStorageId')->willReturn(1);
		$storage = $this->createMock(IStorage::class);
		$storage->method('getCache')->willReturn($cache);
		$storage->method('instanceOfStorage')->willReturnCallback(static fn (string $class): bool => $class === IHomeStorage::class);
		return $storage;
	}

	public function testFileMovedToTheTrashBin() {
		$events = [];
		$listener = new Listener($this->getQueue($events));
		$storage = $this->getHomeStorage();

		// The cache entry moves to the trash bin, users don't see that path
		$listener->cacheListener(new CacheEntryUpdatedEvent($storage, 'files_trashbin/files/report.md.d1760000000', 12, 1));
		$this->assertEquals([], $events);

		$node = $this->createMock(File::class);
		$node->method('getStorage')->willReturn($storage);
		$node->method('getInternalPath')->willReturn('files/Documents/report.md');
		$node->method('getId')->willReturn(12);
		$listener->nodeDeletedListener(new NodeDeletedEvent($node));

		$this->assertEquals([
			'notify_storage_update' => [
				['storage' => 1, 'path' => 'files/Documents/report.md', 'file_id' => 12],
			],
		], $events);
	}

	public function testDeletedNodeWithoutInfo() {
		$events = [];
		$listener = new Listener($this->getQueue($events));

		$node = $this->createMock(File::class);
		$node->method('getStorage')->willReturn($this->getHomeStorage());
		$node->method('getInternalPath')->willReturn('files/report.md');
		$node->method('getId')->willThrowException(new NotFoundException());
		$listener->nodeDeletedListener(new NodeDeletedEvent($node));

		$this->assertEquals([], $events);
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
