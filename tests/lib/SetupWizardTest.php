<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2026 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

namespace OCA\NotifyPush\Tests;

use OCA\NotifyPush\SetupWizard;
use Test\TestCase;

class SetupWizardTest extends TestCase {
	public function httpsUrlProvider(): array {
		return [
			['http://push.example.com', 'https://push.example.com'],
			['http://truc.example.com', 'https://truc.example.com'],
			['http://cloud.example.com', 'https://cloud.example.com'],
			['http://tholos.example.com/nextcloud', 'https://tholos.example.com/nextcloud'],
			['https://cloud.example.com', 'https://cloud.example.com'],
			['cloud.example.com', 'https://cloud.example.com'],
			// the scheme is case insensitive
			['HTTP://push.example.com', 'https://push.example.com'],
			['HTTPS://push.example.com', 'HTTPS://push.example.com'],
			// protocol relative already carries its own separator
			['//push.example.com', 'https://push.example.com'],
		];
	}

	/**
	 * @dataProvider httpsUrlProvider
	 */
	public function testToHttps(string $input, string $expected): void {
		$this->assertEquals($expected, SetupWizard::toHttps($input));
	}
}
