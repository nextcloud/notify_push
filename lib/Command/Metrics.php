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

namespace OCA\NotifyPush\Command;

use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\RedisQueue;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class Metrics extends Command {
	private $queue;

	public function __construct(
		IQueue $queue
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
			$redis->del("notify_push_metrics");
			$this->queue->push("notify_query", "metrics");
			usleep(10 * 1000);
			$metrics = $redis->get("notify_push_metrics");
			if (!$metrics) {
				usleep(100 * 1000);
				$metrics = $redis->get("notify_push_metrics");
			}
			if ($metrics) {
				$metrics = json_decode($metrics, true);
				if (!is_array($metrics)) {
					$output->writeln("<error>Invalid metrics received from push server</error>");
					return 1;
				}
				$output->writeln("Active connection count: " . $metrics['active_connection_count']);
				$output->writeln("Total connection count: " . $metrics['total_connection_count']);
				$output->writeln("Total database query count: " . $metrics['mapping_query_count']);
				$output->writeln("Events received: " . $metrics['events_received']);
				$output->writeln("Messages send: " . $metrics['messages_send']);
				return 0;
			} else {
				$output->writeln("<error>No metrics received from push server</error>");
				return 1;
			}
		} else {
			$output->writeln("<error>Redis is not available</error>");
			return 1;
		}
	}
}
