<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Command;

use OCA\NotifyPush\Queue\IQueue;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class Reset extends Command {
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
			->setName('notify_push:reset')
			->setDescription('Cancel all active connections to the push server');
		parent::configure();
	}

	protected function execute(InputInterface $input, OutputInterface $output): int {
		$this->queue->push('notify_signal', 'reset');
		return 0;
	}
}
