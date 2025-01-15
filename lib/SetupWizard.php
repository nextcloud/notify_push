<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush;

use OCA\NotifyPush\Queue\IQueue;
use OCA\NotifyPush\Queue\RedisQueue;
use OCP\Http\Client\IClientService;
use OCP\IConfig;
use Symfony\Component\Console\Output\BufferedOutput;

class SetupWizard {
	private $queue;
	private $test;
	private $client;
	private $config;
	private $httpsCache = [];
	private $binaryFinder;

	public function __construct(
		IQueue $queue,
		SelfTest $test,
		IClientService $clientService,
		IConfig $config,
		BinaryFinder $binaryFinder,
	) {
		$this->queue = $queue;
		$this->test = $test;
		$this->client = $clientService->newClient();
		$this->config = $config;
		$this->binaryFinder = $binaryFinder;
	}

	public function getArch(): string {
		return $this->binaryFinder->getArch();
	}

	private function getBinaryPath(): string {
		return $this->binaryFinder->getBinaryPath();
	}

	public function hasBundledBinaries(): bool {
		return is_dir(__DIR__ . '/../bin/' . $this->binaryFinder->getArch());
	}

	public function hasBinary(): bool {
		return file_exists($this->getBinaryPath());
	}

	public function testBinary(): bool {
		$path = $this->getBinaryPath();
		@chmod($path, 0755);
		$output = [];
		exec("$path --version", $output);
		return count($output) === 1 && strpos($output[0], 'notify_push') === 0;
	}

	public function isPortFree(): bool {
		$port = 7867;
		return !is_resource(@fsockopen('localhost', $port));
	}

	public function hasRedis(): bool {
		return $this->queue instanceof RedisQueue;
	}

	public function hasSystemd(): bool {
		$result = null;
		$output = [];
		exec('which systemctl 2>&1', $output, $result);
		return $result === 0;
	}

	public function hasSELinux(): bool {
		$result = null;
		$output = [];
		exec('which getenforce 2>&1', $output, $result);
		return $result === 0;
	}

	private function getConfigPath(): string {
		return rtrim(\OC::$configDir, '/') . '/config.php';
	}

	private function getNextcloudUrl(): string {
		$baseUrl = $this->getBaseUrl();
		if (parse_url($baseUrl, PHP_URL_SCHEME) === 'https') {
			$host = parse_url($baseUrl, PHP_URL_HOST);
			// using an ip address and http isn't supported by the push server
			if (substr_count($host, '.') === 3) {
				// since we run the push server on the same server, use localhost instead
				return 'http://localhost/' . parse_url($baseUrl, PHP_URL_PATH);
			}
		}
		return $baseUrl;
	}

	/**
	 * @param bool $selfSigned
	 * @return bool|string
	 */
	public function testAutoConfig(bool $selfSigned) {
		$path = $this->getBinaryPath();
		$config = $this->getConfigPath();
		$descriptorSpec = [
			0 => ['pipe', 'r'],
			1 => ['pipe', 'w'],
			2 => ['pipe', 'w'],
		];
		$pipes = [];
		$proc = proc_open("exec $path $config", $descriptorSpec, $pipes, null, [
			'PORT' => 7867,
			'ALLOW_SELF_SIGNED' => $selfSigned ? 'true' : 'false',
			'LOG' => 'notify_push=info',
			'NEXTCLOUD_URL' => $this->getNextcloudUrl(),
		]);
		// give the server some time to start
		for ($i = 0; $i < 20; $i++) {
			usleep(100 * 1000);
			if ($this->isBinaryRunningAt('localhost:7867')) {
				break;
			}
		}
		$status = proc_get_status($proc);
		if (!$status['running']) {
			proc_terminate($proc);
			rewind($pipes[1]);
			return stream_get_contents($pipes[1]);
		}
		$testResult = $this->selfTestNonProxied(true);
		if ($testResult !== true) {
			proc_terminate($proc);
			rewind($pipes[1]);
			return stream_get_contents($pipes[1]) . $testResult;
		}
		proc_terminate($proc);
		return true;
	}

	public function isBinaryRunningAtDefaultPort(): bool {
		return $this->isBinaryRunningAt('http://localhost:7867');
	}

	private function getBaseUrl(): string {
		$base = $this->config->getSystemValueString('overwrite.cli.url', '');
		if (strpos($base, 'https://') !== 0) {
			$httpsBase = 'https://' . ltrim($base, 'http://');
			if (isset($this->httpsCache[$httpsBase])) {
				return ($this->httpsCache[$httpsBase]) ? $httpsBase : $base;
			}
			try {
				$this->client->get($base, ['nextcloud' => ['allow_local_address' => true], 'verify' => false]);
				$this->httpsCache[$httpsBase] = true;
				return $base;
			} catch (\Exception $e) {
				$this->httpsCache[$httpsBase] = false;
				return $base;
			}
		}
		return $base;
	}

	public function isSelfSigned(): bool {
		$base = $this->getBaseUrl();
		try {
			$this->client->get($base, ['nextcloud' => ['allow_local_address' => true]]);
			return false;
		} catch (\Exception $e) {
			return true;
		}
	}

	public function getProxiedBase(): string {
		return $this->getBaseUrl() . '/push';
	}

	private function isBinaryRunningAt(string $address): bool {
		try {
			$result = $this->client->get($address . '/test/cookie', ['nextcloud' => ['allow_local_address' => true], 'verify' => false]);
			return is_numeric($result->getBody());
		} catch (\Exception $e) {
			return false;
		}
	}

	public function isBinaryRunningBehindProxy(): bool {
		return $this->isBinaryRunningAt($this->getProxiedBase());
	}

	/**
	 * @param bool $ignoreProxyError
	 * @return bool|string
	 */
	public function selfTestNonProxied(bool $ignoreProxyError = false) {
		$output = new BufferedOutput();
		$result = $this->test->test('http://localhost:7867', $output, $ignoreProxyError);
		if ($result === 0) {
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

	public function generateSystemdService(bool $selfSigned): string {
		$path = $this->getBinaryPath();
		$config = $this->getConfigPath();
		$user = posix_getpwuid(posix_getuid())['name'];
		$selfSigned = $selfSigned ? "Environment=ALLOW_SELF_SIGNED=true\n" : '';
		$ncUrl = $this->getNextcloudUrl();
		$service = "[Unit]
Description = Push daemon for Nextcloud clients

[Service]
Environment=PORT=7867
Environment=NEXTCLOUD_URL=$ncUrl
{$selfSigned}ExecStart=$path $config
Type=notify
User=$user

[Install]
WantedBy = multi-user.target
";
		return $service;
	}

	/**
	 * @return false|string
	 */
	public function guessProxy() {
		$base = $this->config->getSystemValueString('overwrite.cli.url', '');
		try {
			$result = $this->client->get($base, ['nextcloud' => ['allow_local_address' => true], 'verify' => false]);
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
		return 'location ^~ /push/ {
	proxy_pass http://127.0.0.1:7867/;
	proxy_http_version 1.1;
	proxy_set_header Upgrade $http_upgrade;
	proxy_set_header Connection "Upgrade";
	proxy_set_header Host $host;
	proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
';
	}

	public function apacheConfig(): string {
		return 'ProxyPass /push/ws ws://127.0.0.1:7867/ws
ProxyPass /push/ http://127.0.0.1:7867/
ProxyPassReverse /push/ http://127.0.0.1:7867/
';
	}
}
