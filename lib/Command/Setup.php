<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Command;

use OCA\NotifyPush\SetupWizard;
use OCP\IConfig;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

class Setup extends Command {
	private $test;
	private $config;
	private $setupWizard;

	public function __construct(
		\OCA\NotifyPush\SelfTest $test,
		IConfig $config,
		SetupWizard $setupWizard,
	) {
		parent::__construct();
		$this->test = $test;
		$this->config = $config;
		$this->setupWizard = $setupWizard;
	}

	/**
	 * @return void
	 */
	protected function configure() {
		$this
			->setName('notify_push:setup')
			->setDescription('Configure push server')
			->addArgument('server', InputArgument::OPTIONAL, 'url of the push server');
		parent::configure();
	}

	protected function execute(InputInterface $input, OutputInterface $output) {
		$server = $input->getArgument('server');
		if ($server) {
			$result = $this->test->test($server, $output);

			if ($result === 0) {
				$this->config->setAppValue('notify_push', 'base_endpoint', $server);
				$output->writeln('  configuration saved');
			}
			return $result;
		} else {
			if (!$this->setupWizard->hasBundledBinaries()) {
				$output->writeln('<error>🗴 bundled binaries are not available.</error>');
				$output->writeln("  If you're trying to setup the app from git, you can find build instruction in the README: https://github.com/nextcloud/notify_push");
				$output->writeln('  And pre-built binaries for x86_64, armv7, aarch64 and freebsd (amd64) in the github actions.');
				$output->writeln('  Once you have a <info>notify_push</info> binary it should be placed in <info>' . realpath(__DIR__ . '/../../') . '/bin/' . $this->setupWizard->getArch()) . '</info>';
				return 1;
			}

			$output->writeln('This setup wizard is intended for use on single server instances');
			$output->writeln('where the nextcloud server, web server/reverse proxy and push daemon all run on the same machine.');
			$output->writeln('If your setup is more complex or involves any kind of load balancing');
			$output->writeln('you should follow the manual setup instruction on the README instead');
			$output->writeln('<info>https://github.com/nextcloud/notify_push</info>');

			if (!$this->enterToContinue($output)) {
				return 0;
			}

			if (!$this->setupWizard->hasRedis()) {
				$output->writeln('<error>🗴 redis is required.</error>');
				return 1;
			}

			$url = $this->config->getSystemValueString('overwrite.cli.url', '');
			if (!$url) {
				$output->writeln("<error>🗴 'overwrite.cli.url' needs to be configured in your system config.</error>");
				$output->writeln('  See https://docs.nextcloud.com/server/latest/admin_manual/configuration_server/config_sample_php_parameters.html#proxy-configurations');
			}
			if (strpos($url, 'localhost') !== false) {
				$output->writeln("<comment>🗴 'overwrite.cli.url' is set to localhost, the push server will not be reachable from other machines</comment>");
			}

			if (!$this->setupWizard->hasBinary()) {
				$output->writeln('<error>🗴 your system architecture(' . $this->setupWizard->getArch() . ') is not supported by the bundled binaries.</error>');
				$output->writeln('  you can find build instructions for the notify_push binary in the README: https://github.com/nextcloud/notify_push');
				$output->writeln('  And pre-built binaries for x86_64, armv7, aarch64 and freebsd (amd64) in the github actions.');
				$output->writeln('  Once you have a <info>notify_push</info> binary it should be placed in <info>' . realpath(__DIR__ . '/../../bin/' . $this->setupWizard->getArch())) . '</info>';
				return 1;
			}

			if (!$this->setupWizard->testBinary()) {
				$output->writeln('<error>🗴 bundled binary not working on your system.</error>');
				if ($this->setupWizard->hasSELinux()) {
					$output->writeln('  It looks like your system has SELinux enabled which might be blocking execution of the binary.');
				}
				$this->readmeLink($output);
				return 1;
			}

			if (!$this->setupWizard->hasSystemd()) {
				$output->writeln("<error>🗴 your system doesn't seem to be using systemd.</error>");
				$output->writeln('  You can still use the app without systemd by following the manual setup instructions.');
				$this->readmeLink($output);
				return 1;
			}

			$trustedProxies = $this->config->getSystemValue('trusted_proxies', []);
			if (array_search('127.0.0.1', $trustedProxies) === false) {
				$trustedProxies[] = '127.0.0.1';
				$this->config->setSystemValue('trusted_proxies', $trustedProxies);
			}
			if (array_search('::1', $trustedProxies) === false) {
				$trustedProxies[] = '::1';
				$this->config->setSystemValue('trusted_proxies', $trustedProxies);
			}

			if (!$this->setupWizard->isBinaryRunningAtDefaultPort()) {
				if (!$this->setupWizard->isPortFree()) {
					$output->writeln('<error>🗴 default port(7867) is in use.</error>');
					$output->writeln("  if you've already setup the notify_push binary then call the setup command with the address it's listening on.");
					$this->readmeLink($output);
					return 1;
				}

				$selfSigned = $this->setupWizard->isSelfSigned();
				if ($selfSigned) {
					$output->writeln('<comment>  Allowing self-signed certificates in the push config.</comment>');
				}

				$testResult = $this->setupWizard->testAutoConfig($selfSigned);
				if ($testResult !== true) {
					$output->writeln('<error>🗴 failed to run self-test with auto-generated config.</error>');
					if (is_string($testResult)) {
						$this->printTestResult($output, $testResult);
					}
					$this->readmeLink($output);
					return 1;
				}

				$systemd = $this->setupWizard->generateSystemdService($selfSigned);

				$output->writeln('Place the following systemd config at <info>/etc/systemd/system/notify_push.service</info>');
				$output->writeln('');
				$output->writeln($systemd);
				$output->writeln('');
				$output->writeln('And run <info>sudo systemctl enable --now notify_push</info>');

				if (!$this->enterToContinue($output)) {
					return 0;
				}

				if (!$this->setupWizard->isBinaryRunningAtDefaultPort()) {
					$output->writeln("<error>🗴 push binary doesn't seem to be running, did you follow the above instructions?.</error>");
					$this->readmeLink($output);
					return 1;
				}
			} else {
				$output->writeln('Push binary seems to be running already');
			}

			$testResult = $this->setupWizard->selfTestNonProxied();
			if ($testResult !== true) {
				$output->writeln('<error>🗴 failed to run self-test.</error>');
				if (is_string($testResult)) {
					$this->printTestResult($output, $testResult);
				}
				$this->readmeLink($output);
				return 1;
			}
			$output->writeln('<info>✓ push server seems to be functioning correctly.</info>');

			if (!$this->setupWizard->isBinaryRunningBehindProxy()) {
				$proxy = $this->setupWizard->guessProxy();
				if ($proxy === 'nginx') {
					$output->writeln('Place the following nginx config within the <info>server</info> block of the nginx config for your nextcloud installation');
					$output->writeln('which can usually be found within <info>/etc/nginx/sites-enabled/</info>');
					$output->writeln('');
					$output->writeln($this->setupWizard->nginxConfig());
					$output->writeln('');
					$output->writeln('And reload the config using <info>sudo nginx -s reload</info>');
				} elseif ($proxy === 'apache') {
					$output->writeln('Run the following commands to enable the proxy modules');
					$output->writeln('    <info>sudo a2enmod proxy</info>');
					$output->writeln('    <info>sudo a2enmod proxy_http</info>');
					$output->writeln('    <info>sudo a2enmod proxy_wstunnel</info>');
					$output->writeln('');
					$output->writeln('Then place the following within the <info><VirtualHost></info> block of the apache config for your nextcloud installation');
					$output->writeln('which can usually be found within <info>/etc/apache2/sites-enabled/</info>');
					$output->writeln('Note that there might be both an <info>http</info> and <info>https</info> config file');
					$output->writeln('');
					$output->writeln($this->setupWizard->apacheConfig());
					$output->writeln('');
					$output->writeln('And reload apache using <info>sudo systemctl restart apache2</info>');
				} else {
					$output->writeln('<error>🗴 failed to detect reverse proxy.</error>');
					$this->readmeLink($output);
					return 1;
				}
				if (!$this->enterToContinue($output)) {
					return 0;
				}

				if (!$this->setupWizard->isBinaryRunningBehindProxy()) {
					$output->writeln("<error>🗴 push binary doesn't seem to be reachable through the reverse proxy, did you follow the above instructions?.</error>");
					$this->readmeLink($output);
					return 1;
				}
			} else {
				$output->writeln('Reverse proxy seems to be configured already');
			}

			$testResult = $this->setupWizard->selfTestProxied();
			if ($testResult !== true) {
				$output->writeln('<error>🗴 failed to run self-test.</error>');
				if (is_string($testResult)) {
					$this->printTestResult($output, $testResult);
				}
				$this->readmeLink($output);
				return 1;
			}

			$output->writeln('<info>✓ reverse proxy seems to be setup correctly.</info>');
			$this->config->setAppValue('notify_push', 'base_endpoint', $this->setupWizard->getProxiedBase());
			$output->writeln('  configuration saved');
		}

		return 0;
	}

	private function readmeLink(OutputInterface $output): void {
		$output->writeln('  See the steps in the README for manual setup instructions: https://github.com/nextcloud/notify_push');
	}

	private function printTestResult(OutputInterface $output, string $result): void {
		$lines = explode("\n", $result);
		foreach ($lines as $i => &$line) {
			if ($i === 0) {
				$line = 'test output: ' . $line;
			} else {
				$line = '             ' . $line;
			}
			$output->writeln($line);
		}
	}

	private function enterToContinue(OutputInterface $output): bool {
		$output->write('Press enter to continue or ESC to cancel...');
		system('stty cbreak');
		$result = null;
		while ($result === null) {
			$key = fgetc(STDIN);
			if ($key === "\e") {
				$result = false;
			} elseif ($key === "\n") {
				$result = true;
			}
			usleep(100);
		}
		system('stty -cbreak');
		$output->writeln('');
		return $result;
	}
}
