<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Command;

use OCA\NotifyPush\Queue\IQueue;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;

class Log extends Command {
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
	protected function configure(): void {
		$this
			->setName('notify_push:log')
			->setDescription('Temporarily set the log level of the push server')
			->addOption('restore', 'r', InputOption::VALUE_NONE, 'restore the log level to the previous value')
			->addArgument('level', InputArgument::OPTIONAL, 'the new log level to set');
		parent::configure();
	}

	protected function execute(InputInterface $input, OutputInterface $output): int {
		$level = $input->getArgument('level');
		if ($input->getOption('restore')) {
			$output->writeln('restoring log level');
			$this->queue->push('notify_config', 'log_restore');
		} elseif ($level) {
			// by default dont touch the log level of the libraries
			if (!strpos($level, '=') and $level !== 'trace') {
				$level = "notify_push=$level";
			}
			$this->queue->push('notify_config', ['log_spec' => $level]);
		}
		return 0;
	}
}
