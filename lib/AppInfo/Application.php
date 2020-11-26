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

namespace OCA\NotifyPush\AppInfo;

use OC\AppFramework\Utility\SimpleContainer;
use OC\RedisFactory;
use OCA\NotifyPush\Capabilities;
use OCA\NotifyPush\Listener;
use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\NullQueue;
use OCA\NotifyPush\Queue\RedisQueue;
use OCP\AppFramework\App;
use OCP\AppFramework\Bootstrap\IBootContext;
use OCP\AppFramework\Bootstrap\IBootstrap;
use OCP\AppFramework\Bootstrap\IRegistrationContext;
use OCP\EventDispatcher\IEventDispatcher;
use OCP\Files\Cache\CacheInsertEvent;
use OCP\Files\Cache\CacheEntryRemovedEvent;
use OCP\Files\Cache\CacheUpdateEvent;

class Application extends App implements IBootstrap {
	public const APP_ID = 'notify_push';

	public function __construct() {
		parent::__construct(self::APP_ID);
	}

	public function register(IRegistrationContext $context): void {
		$context->registerCapability(Capabilities::class);

		$context->registerService(IQueue::class, function(SimpleContainer $c) {
			/** @var RedisFactory $redisFactory */
			$redisFactory = $c->get(RedisFactory::class);
			if ($redisFactory->isAvailable()) {
				return new RedisQueue($redisFactory->getInstance());
			} else {
				return new NullQueue();
			}
		});
	}

	public function boot(IBootContext $context): void {
		$context->injectFn([$this, 'attachHooks']);
	}

	public function attachHooks(
		IEventDispatcher $eventDispatcher,
		Listener $listener
	) {
		$eventDispatcher->addListener(CacheInsertEvent::class, [$listener, 'cacheListener']);
		$eventDispatcher->addListener(CacheUpdateEvent::class, [$listener, 'cacheListener']);
		$eventDispatcher->addListener(CacheEntryRemovedEvent::class, [$listener, 'cacheListener']);
	}
}
