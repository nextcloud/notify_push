<?php

namespace OC\Core\Command;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

/**
 * Base class for Nextcloud commands with output formatting support
 */
class Base extends Command {
	public const OUTPUT_FORMAT_PLAIN = 'plain';
	public const OUTPUT_FORMAT_JSON = 'json';
	public const OUTPUT_FORMAT_JSON_PRETTY = 'json_pretty';

	protected string $defaultOutputFormat = self::OUTPUT_FORMAT_PLAIN;

	protected function writeArrayInOutputFormat(InputInterface $input, OutputInterface $output, iterable $items, string $prefix = '  - '): void {
	}

	protected function writeTableInOutputFormat(InputInterface $input, OutputInterface $output, array $items): void {
	}

	protected function writeStreamingTableInOutputFormat(InputInterface $input, OutputInterface $output, \Iterator $items, int $tableGroupSize): void {
	}

	protected function writeStreamingJsonArray(InputInterface $input, OutputInterface $output, \Iterator $items): void {
	}

	public function chunkIterator(\Iterator $iterator, int $count): \Iterator {
	}
}
