<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Command;

use OCP\IConfig;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class SelfTest extends Command {
	private $test;
	private $config;

	public function __construct(
		\OCA\NotifyPush\SelfTest $test,
		IConfig $config,
	) {
		parent::__construct();
		$this->test = $test;
		$this->config = $config;
	}


	/**
	 * @return void
	 */
	protected function configure() {
		$this
			->setName('notify_push:self-test')
			->setDescription('Run self test for configured push server');
		parent::configure();
	}

	protected function execute(InputInterface $input, OutputInterface $output) {
		$server = $this->config->getAppValue('notify_push', 'base_endpoint', '');
		if (!$server) {
			$output->writeln('<error>🗴 no push server configured</error>');
			return 1;
		}
		return $this->test->test($server, $output);
	}
}
