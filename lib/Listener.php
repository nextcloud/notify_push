<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush;

use OC\Files\Storage\Wrapper\Jail;
use OCA\NotifyPush\Queue\IQueue;
use OCP\Activity\IConsumer;
use OCP\Activity\IEvent;
use OCP\EventDispatcher\Event;
use OCP\Files\Cache\ICacheEvent;
use OCP\Files\IHomeStorage;
use OCP\Files\Storage\IStorage;
use OCP\Group\Events\UserAddedEvent;
use OCP\Group\Events\UserRemovedEvent;
use OCP\Notification\IApp;
use OCP\Notification\IDismissableNotifier;
use OCP\Notification\INotification;
use OCP\Notification\INotifier;
use OCP\Share\Events\ShareCreatedEvent;
use OCP\Share\IShare;

class Listener implements IConsumer, IApp, INotifier, IDismissableNotifier {
	private IQueue $queue;

	public function __construct(IQueue $queue) {
		$this->queue = $queue;
	}

	public function cacheListener(Event $event): void {
		if ($event instanceof ICacheEvent) {
			$path = $event->getPath();

			$storage = $event->getStorage();
			while ($storage->instanceOfStorage(Jail::class)) {
				/** @var Jail $storage */
				$path = $storage->getUnjailedPath($path);
				$storage = $storage->getUnjailedStorage();
			}

			if ($this->shouldNotifyPath($event->getStorage(), $path)) {
				$this->queue->push('notify_storage_update', [
					'storage' => $event->getStorageId(),
					'path' => $path,
					'file_id' => $event->getFileId(),
				]);
			}
		}
	}

	/***
	 * @param UserAddedEvent|UserRemovedEvent $event
	 */
	public function groupListener($event): void {
		$this->queue->push('notify_group_membership_update', [
			'user' => $event->getUser()->getUID(),
			'group' => $event->getGroup()->getGID(),
		]);
	}

	public function shareListener(ShareCreatedEvent $event): void {
		$share = $event->getShare();

		if ($share->getShareType() === IShare::TYPE_USER) {
			$this->queue->push('notify_user_share_created', [
				'user' => $share->getSharedWith(),
			]);
		}
		// todo group shares
	}

	public function receive(IEvent $event) {
		$this->queue->push('notify_activity', [
			'user' => $event->getAffectedUser(),
		]);
	}

	public function notify(INotification $notification): void {
		$this->queue->push('notify_notification', [
			'user' => $notification->getUser(),
		]);
	}

	public function markProcessed(INotification $notification): void {
	}

	public function getCount(INotification $notification): int {
		return 0;
	}

	public function getID(): string {
		return 'notify_push';
	}

	public function getName(): string {
		return 'notify_push';
	}

	public function prepare(INotification $notification, string $languageCode): INotification {
		throw new \InvalidArgumentException();
	}

	public function dismissNotification(INotification $notification): void {
		$this->queue->push('notify_notification', [
			'user' => $notification->getUser(),
		]);
	}

	private function shouldNotifyPath(IStorage $storage, string $path): bool {
		// ignore files in home storage but outside home directory (trashbin, versions, etc)
		if (
			$storage->instanceOfStorage(IHomeStorage::class)) {
			return $path === 'files' || str_starts_with($path, 'files/');
		}

		// ignore appdata
		if (str_starts_with($path, 'appdata_')) {
			return false;
		}

		if ($path === '__groupfolders') {
			return false;
		}
		if (str_starts_with($path, '__groupfolders/versions')) {
			return false;
		}
		if (str_starts_with($path, '__groupfolders/trash')) {
			return false;
		}

		return true;
	}
}
