<?php

// some comment

$CONFIG = [
	'overwrite.cli.url' => 'https://cloud.example.com',
	'dbtype' => 'mysql', // comment inside the config
	'dbname' => 'nextcloud',
	'dbhost' => '127.0.0.1',
	/**
	 * block comment
	 */
	'dbport' => '',
	'dbtableprefix' => 'oc_',
	'dbuser' => 'nextcloud',
	'dbpassword' => 'secret',
	'redis' => [
		'host' => 'localhost'
	]
];
