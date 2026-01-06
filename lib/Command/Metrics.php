<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Command;

use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\RedisQueue;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class Metrics extends Command {
	private $queue;

	public function __construct(
		IQueue $queue,
	) {
		parent::__construct();
		$this->queue = $queue;
	}

	/**
	 * @return void
	 */
	protected function configure() {
		$this
			->setName('notify_push:metrics')
			->setDescription('Get the metrics from the push server');
		parent::configure();
	}

	protected function execute(InputInterface $input, OutputInterface $output): int {
		if ($this->queue instanceof RedisQueue) {
			$redis = $this->queue->getConnection();
			$redis->del('notify_push_metrics');
			$this->queue->push('notify_query', 'metrics');
			usleep(10 * 1000);
			$metrics = $redis->get('notify_push_metrics');
			if (!$metrics) {
				usleep(100 * 1000);
				$metrics = $redis->get('notify_push_metrics');
			}
			if ($metrics) {
				$metrics = json_decode($metrics, true);
				if (!is_array($metrics)) {
					$output->writeln('<error>Invalid metrics received from push server</error>');
					return 1;
				}
				$output->writeln('Active connection count: ' . $metrics['active_connection_count']);
				$output->writeln('Active user count: ' . $metrics['active_user_count']);
				$output->writeln('Total connection count: ' . $metrics['total_connection_count']);
				$output->writeln('Total database query count: ' . $metrics['mapping_query_count']);
				$output->writeln('Events received: ' . $metrics['events_received']);
				$output->writeln('Messages sent: ' . $metrics['messages_sent']);
				$output->writeln('Messages sent (file): ' . $metrics['messages_sent_file']);
				$output->writeln('Messages sent (notification): ' . $metrics['messages_sent_notification']);
				$output->writeln('Messages sent (activity): ' . $metrics['messages_sent_activity']);
				$output->writeln('Messages sent (custom): ' . $metrics['messages_sent_custom']);
				return 0;
			} else {
				$output->writeln('<error>No metrics received from push server</error>');
				return 1;
			}
		} else {
			$output->writeln('<error>Redis is not available</error>');
			return 1;
		}
	}
}
