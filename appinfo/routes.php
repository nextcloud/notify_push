<?php

declare(strict_types=1);
/**
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

return [
	'routes' => [
		[
			'name' => 'test#cookie',
			'url' => '/test/cookie',
			'verb' => 'GET',
		],
		[
			'name' => 'test#remote',
			'url' => '/test/remote',
			'verb' => 'GET',
		],
		[
			'name' => 'test#version',
			'url' => '/test/version',
			'verb' => 'GET',
		],
		[
			'name' => 'Auth#preAuth',
			'url' => '/pre_auth',
			'verb' => 'POST',
		],
		[
			'name' => 'Auth#getUid',
			'url' => '/uid',
		],
	],
];
