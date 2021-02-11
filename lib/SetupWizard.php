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

namespace OCA\NotifyPush;

use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\RedisQueue;
use OCP\App\IAppManager;
use OCP\Http\Client\IClientService;
use OCP\IConfig;
use Symfony\Component\Console\Output\BufferedOutput;

class SetupWizard {
	private $appManager;
	private $queue;
	private $test;
	private $client;
	private $config;

	public function __construct(
		IAppManager $appManager,
		IQueue $queue,
		SelfTest $test,
		IClientService $clientService,
		IConfig $config
	) {
		$this->appManager = $appManager;
		$this->queue = $queue;
		$this->test = $test;
		$this->client = $clientService->newClient();
		$this->config = $config;
	}

	public function getArch(): string {
		$arch = php_uname('m');
		if (strpos($arch, 'armv7') === 0) {
			return 'armv7';
		}
		if (strpos($arch, 'aarch64') === 0) {
			return 'aarch64';
		}
		return $arch;
	}

	private function getBinaryPath(): string {
		$basePath = realpath(__DIR__ . '/../bin/');
		$arch = $this->getArch();
		return "$basePath/$arch/notify_push";
	}

	public function hasBundledBinaries() {
		return is_dir(__DIR__ . '/../bin/x86_64');
	}

	public function hasBinary(): bool {
		return file_exists($this->getBinaryPath());
	}

	public function testBinary(): bool {
		$path = $this->getBinaryPath();
		$appVersion = $this->appManager->getAppVersion("notify_push");
		$output = [];
		exec("$path --version", $output);
		return count($output) === 1 && $output[0] === "notify_push $appVersion";
	}

	public function isPortFree() {
		$port = 7867;
		return !is_resource(@fsockopen('localhost', $port));
	}

	public function hasRedis() {
		return $this->queue instanceof RedisQueue;
	}

	public function hasSystemd() {
		$result = null;
		$output = [];
		exec("which systemctl 2>&1", $output, $result);
		return $result === 0;
	}

	public function hasSELinux() {
		$result = null;
		$output = [];
		exec("which getenforce 2>&1", $output, $result);
		return $result === 0;
	}

	private function getConfigPath() {
		return rtrim(\OC::$configDir, '/') . '/config.php';
	}

	/**
	 * @return bool|string
	 */
	public function testAutoConfig() {
		$path = $this->getBinaryPath();
		$config = $this->getConfigPath();
		$descriptorSpec = [
			0 => ["pipe", "r"],
			1 => ["pipe", "w"],
		];
		$pipes = [];
		$proc = proc_open("exec $path $config", $descriptorSpec, $pipes, null, [
			'PORT' => 7867,
		]);
		// give the server some time to start
		usleep(100 * 1000);
		$status = proc_get_status($proc);
		if (!$status['running']) {
			proc_terminate($proc);
			return false;
		}
		$testResult = $this->selfTestNonProxied();
		if ($testResult !== true) {
			proc_terminate($proc);
			return $testResult;
		}
		proc_terminate($proc);
		return true;
	}

	public function isBinaryRunningAtDefaultPort(): bool {
		try {
			$result = $this->client->get("http://localhost:7867/test/cookie", ['nextcloud' => ['allow_local_address' => true]]);
			return is_numeric($result->getBody());
		} catch (\Exception $e) {
			return false;
		}
	}

	public function getProxiedBase(): string {
		$base = $this->config->getSystemValueString('overwrite.cli.url', '');
		return $base . '/push';
	}

	public function isBinaryRunningBehindProxy(): bool {
		try {
			$result = $this->client->get($this->getProxiedBase() . "/test/cookie", ['nextcloud' => ['allow_local_address' => true]]);
			return is_numeric($result->getBody());
		} catch (\Exception $e) {
			return false;
		}
	}

	/**
	 * @return bool|string
	 */
	public function selfTestNonProxied() {
		$output = new BufferedOutput();
		if ($this->test->test("http://localhost:7867", $output) === 0) {
			return true;
		} else {
			return $output->fetch();
		}
	}

	/**
	 * @return bool|string
	 */
	public function selfTestProxied() {
		$output = new BufferedOutput();
		if ($this->test->test($this->getProxiedBase(), $output) === 0) {
			return true;
		} else {
			return $output->fetch();
		}
	}

	public function generateSystemdService() {
		$path = $this->getBinaryPath();
		$config = $this->getConfigPath();
		$user = posix_getpwuid(posix_getuid())['name'];
		$service = "[Unit]
Description = Push daemon for Nextcloud clients

[Service]
Environment=PORT=7867
ExecStart=$path $config
User=$user

[Install]
WantedBy = multi-user.target
";
		return $service;
	}

	public function guessProxy() {
		$base = $this->config->getSystemValueString('overwrite.cli.url', '');
		try {
			$result = $this->client->get($base, ['nextcloud' => ['allow_local_address' => true]]);
			$server = strtolower($result->getHeader('server'));
			if (strpos($server, 'apache') !== false) {
				return 'apache';
			} elseif (strpos($server, 'nginx') !== false) {
				return 'nginx';
			}
			return false;
		} catch (\Exception $e) {
			return false;
		}
	}

	public function nginxConfig(): string {
		return "location /push/ {
	proxy_pass http://localhost:7867/;
	proxy_http_version 1.1;
	proxy_set_header Upgrade \$http_upgrade;
	proxy_set_header Connection \"Upgrade\";
	proxy_set_header Host \$host;
	proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
}
";
	}

	public function apacheConfig(): string {
		return "ProxyPass /push/ http://localhost:7867/
ProxyPassReverse /push/ http://localhost:7867/
";
	}
}
