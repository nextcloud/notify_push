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
