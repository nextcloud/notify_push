<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\AppInfo;

use OCA\NotifyPush\Capabilities;
use OCA\NotifyPush\CSPListener;
use OCA\NotifyPush\Listener;
use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\NullQueue;
use OCA\NotifyPush\Queue\PushRedisFactory;
use OCA\NotifyPush\Queue\RedisQueue;
use OCP\Activity\IManager;
use OCP\AppFramework\App;
use OCP\AppFramework\Bootstrap\IBootContext;
use OCP\AppFramework\Bootstrap\IBootstrap;
use OCP\AppFramework\Bootstrap\IRegistrationContext;
use OCP\EventDispatcher\IEventDispatcher;
use OCP\Files\Cache\CacheEntryInsertedEvent;
use OCP\Files\Cache\CacheEntryRemovedEvent;
use OCP\Files\Cache\CacheEntryUpdatedEvent;
use OCP\Group\Events\UserAddedEvent;
use OCP\Group\Events\UserRemovedEvent;
use OCP\Security\CSP\AddContentSecurityPolicyEvent;
use OCP\Share\Events\ShareCreatedEvent;
use Psr\Container\ContainerInterface;

class Application extends App implements IBootstrap {
	public const APP_ID = 'notify_push';

	public function __construct() {
		parent::__construct(self::APP_ID);
	}

	public function register(IRegistrationContext $context): void {
		$context->registerCapability(Capabilities::class);

		$context->registerService(IQueue::class, function (ContainerInterface $c) {
			/** @var PushRedisFactory $factory */
			$factory = $c->get(PushRedisFactory::class);
			$redis = $factory->getRedis();
			if ($redis) {
				return new RedisQueue($redis);
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
		Listener $listener,
		IManager $activityManager,
		\OCP\Notification\IManager $notificationManager,
	): void {
		$eventDispatcher->addServiceListener(AddContentSecurityPolicyEvent::class, CSPListener::class);

		$eventDispatcher->addListener(CacheEntryInsertedEvent::class, [$listener, 'cacheListener']);
		$eventDispatcher->addListener(CacheEntryUpdatedEvent::class, [$listener, 'cacheListener']);
		$eventDispatcher->addListener(CacheEntryRemovedEvent::class, [$listener, 'cacheListener']);

		$eventDispatcher->addListener(UserAddedEvent::class, [$listener, 'groupListener']);
		$eventDispatcher->addListener(UserRemovedEvent::class, [$listener, 'groupListener']);

		$eventDispatcher->addListener(ShareCreatedEvent::class, [$listener, 'shareListener']);

		$activityManager->registerConsumer(function () use ($listener) {
			return $listener;
		});

		$notificationManager->registerApp(Listener::class);
		$notificationManager->registerNotifierService(Listener::class);
	}
}
