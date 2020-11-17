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

use OC\Cache\CappedMemoryCache;
use OCA\NotifyPush\Queue\IQueue;
use OCP\Files\Cache\ICacheEvent;

class Listener {
	private $queue;

	private $sendUpdates;

	public function __construct(IQueue $queue) {
		$this->queue = $queue;
		$this->sendUpdates = new CappedMemoryCache();
	}

	public function cacheListener(ICacheEvent $event) {
		$storage = $event->getStorageId();
		$key = $storage . '::' . $event->getPath();
		if ($this->sendUpdates[$key]) {
			return;
		}
		$this->sendUpdates[$key] = true;

		$this->queue->push('notify_storage_update', [
			'storage' => $event->getStorageId(),
			'path' => $event->getPath(),
		]);
	}
}
