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

use OC\Core\Command\Base;
use OCA\NotifyPush\Queue\IQueue;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;

class Log extends Base {
	private $queue;

	public function __construct(
		IQueue $queue,
	) {
		parent::__construct();
		$this->queue = $queue;
	}


	protected function configure() {
		$this
			->setName('notify_push:log')
			->setDescription('Temporarily set the log level of the push server')
			->addOption("restore", "r", InputOption::VALUE_NONE, "restore the log level to the previous value")
			->addArgument("level", InputArgument::OPTIONAL, "the new log level to set");
		parent::configure();
	}

	protected function execute(InputInterface $input, OutputInterface $output) {
		$level = $input->getArgument("level");
		if ($input->getOption("restore")) {
			$output->writeln("restoring log level");
			$this->queue->push("notify_config", "log_restore");
		} elseif ($level) {
			// by default dont touch the log level of the libraries
			if (!strpos($level, "=") and $level !== "trace") {
				$level = "notify_push=$level";
			}
			$this->queue->push("notify_config", ["log_spec" => $level]);
		}
		return 0;
	}
}

