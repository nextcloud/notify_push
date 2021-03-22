<?php

$CONFIG = [
	'overwrite.cli.url' => 'https://cloud.example.com',
	'dbtype' => 'mysql',
	'dbname' => 'nextcloud',
	'dbhost' => '127.0.0.1',
	'dbport' => '',
	'dbtableprefix' => 'oc_',
	'dbuser' => 'nextcloud',
	'dbpassword' => 'secret',
     'redis.cluster' =>
      array (
        'seeds' =>
        array (
          0 => 'db1:6380',
          1 => 'db1:6381',
          2 => 'db1:6382',
          3 => 'db2:6380',
          4 => 'db2:6381',
          5 => 'db2:6382',
        ),
        'password' => 'xxx',
        'timeout' => 0.0,
        'read_timeout' => 0.0,
        'failover_mode' => 1,
      ),
];
