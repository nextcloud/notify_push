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
use OCP\Activity\IConsumer;
use OCP\Activity\IEvent;
use OCP\EventDispatcher\Event;
use OCP\Files\Cache\ICacheEvent;
use OCP\Files\IHomeStorage;
use OCP\Group\Events\UserAddedEvent;
use OCP\Group\Events\UserRemovedEvent;
use OCP\Notification\IApp;
use OCP\Notification\IDismissableNotifier;
use OCP\Notification\INotification;
use OCP\Notification\INotifier;
use OCP\Share\Events\ShareCreatedEvent;
use OCP\Share\IShare;

class Listener implements IConsumer, IApp, INotifier, IDismissableNotifier {
	private $queue;

	public function __construct(IQueue $queue) {
		$this->queue = $queue;
	}

	public function cacheListener(Event $event): void {
		if ($event instanceof ICacheEvent) {
			// ignore files in home storage but outside home directory (trashbin, versions, etc)
			if (
				$event->getStorage()->instanceOfStorage(IHomeStorage::class) && !(
					$event->getPath() === 'files' || strpos($event->getPath(), "files/") === 0
				)
			) {
				return;
			}
			// ignore appdata
			if (strpos($event->getPath(), 'appdata_') === 0) {
				return;
			}
			$this->queue->push('notify_storage_update', [
				'storage' => $event->getStorageId(),
				'path' => $event->getPath(),
				'file_id' => $event->getFileId(),
			]);
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
}
