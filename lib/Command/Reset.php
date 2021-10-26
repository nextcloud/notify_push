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
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class Reset extends Command {
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
			->setName('notify_push:reset')
			->setDescription('Cancel all active connections to the push server');
		parent::configure();
	}

	protected function execute(InputInterface $input, OutputInterface $output) {
		$this->queue->push("notify_signal", "reset");
		return 0;
	}
}
