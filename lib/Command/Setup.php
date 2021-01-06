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

namespace OCA\NotifyPush\Command;

use OC\Core\Command\Base;
use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\SelfTest;
use OCP\Files\Config\IUserMountCache;
use OCP\Http\Client\IClientService;
use OCP\IConfig;
use OCP\IDBConnection;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class Setup extends Base {
	private $config;
	private $queue;
	private $clientService;
	private $mountCache;
	private $connection;

	public function __construct(
		IConfig $config,
		IQueue $queue,
		IClientService $clientService,
		IUserMountCache $mountCache,
		IDBConnection $connection
	) {
		parent::__construct();
		$this->config = $config;
		$this->queue = $queue;
		$this->clientService = $clientService;
		$this->mountCache = $mountCache;
		$this->connection = $connection;
	}


	protected function configure() {
		$this
			->setName('notify_push:setup')
			->setDescription('Configure push server')
			->addArgument('server', InputArgument::REQUIRED, "url of the push server");
		parent::configure();
	}

	protected function execute(InputInterface $input, OutputInterface $output) {
		$server = $input->getArgument('server');
		
		$test = new SelfTest(
			$this->clientService->newClient(),
			$this->config,
			$this->queue,
			$this->connection,
			$server
		);
		$result = $test->test($output);

		if ($result === 0) {
			$this->config->setAppValue('notify_push', 'base_endpoint', $server);
			$output->writeln("  configuration saved");
		}

		return $result;
	}
}
