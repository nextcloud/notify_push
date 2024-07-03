<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Tests;

use OC\Files\Storage\Temporary;
use OCA\NotifyPush\AppInfo\Application;
use OCA\NotifyPush\Listener;
use OCA\NotifyPush\Queue\IQueue;
use OCP\Activity\IManager as IActivityManager;
use OCP\EventDispatcher\IEventDispatcher;
use OCP\IGroupManager;
use OCP\IUserManager;
use OCP\Notification\IManager as INotificationManager;
use Test\TestCase;

/**
 * @group DB
 */
class CoreEventsTest extends TestCase {
	private function getListener(array &$events) {
		$queue = $this->createMock(IQueue::class);
		$queue->method('push')->willReturnCallback(function ($channel, $event) use (&$events) {
			if (!isset($events[$channel])) {
				$events[$channel] = [];
			}
			$events[$channel][] = $event;
		});
		$listener = new Listener($queue);
		$app = \OC::$server->get(Application::class);
		$app->attachHooks(\OC::$server->get(IEventDispatcher::class), $listener, \OC::$server->get(IActivityManager::class), \OC::$server->get(INotificationManager::class));
		return $listener;
	}

	public function testFilesystemEvents() {
		$storage = new Temporary([]);
		$cache = $storage->getCache();
		$scanner = $storage->getScanner();

		$storage->mkdir('foobar');
		$scanner->scan('');

		$events = [];
		$this->getListener($events);

		$storage->touch('foobar', 100);
		$storage->getUpdater()->update('foobar');

		// file ids are unstable, so we remove them
		foreach ($events['notify_storage_update'] as &$event) {
			unset($event['file_id']);
		}

		$this->assertEquals([
			'notify_storage_update' => [
				['storage' => $cache->getNumericStorageId(), 'path' => 'foobar'],
				['storage' => $cache->getNumericStorageId(), 'path' => 'foobar'],
				['storage' => $cache->getNumericStorageId(), 'path' => ''],
			],
		], $events);
	}

	public function testGroupEvents() {
		$userManager = \OC::$server->get(IUserManager::class);
		$groupManager = \OC::$server->get(IGroupManager::class);
		$uid = uniqid('user_');
		$gid = uniqid('user_');

		$groupManager->createGroup($gid);
		$userManager->createUser($uid, 'a');
		$group = $groupManager->get($gid);
		$user = $userManager->get($uid);

		$events = [];
		$this->getListener($events);

		$group->addUser($user);

		$this->assertEquals([
			'notify_group_membership_update' => [
				['user' => $uid, 'group' => $gid],
			],
			'notify_activity' => [
				['user' => $uid],
			],
		], $events);

		$events = [];

		$group->removeUser($user);

		$this->assertEquals([
			'notify_group_membership_update' => [
				['user' => $uid, 'group' => $gid],
			],
			'notify_activity' => [
				['user' => $uid],
			],
		], $events);
	}
}
