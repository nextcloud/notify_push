<?php

/*
 * This file is part of the Symfony package.
 *
 * (c) Fabien Potencier <fabien@symfony.com>
 *
 * For the full copyright and license information, please view the LICENSE
 * file that was distributed with this source code.
 */

namespace Symfony\Component\Console\Input;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Completion\CompletionInput;
use Symfony\Component\Console\Completion\CompletionSuggestions;
use Symfony\Component\Console\Completion\Suggestion;
use Symfony\Component\Console\Exception\InvalidArgumentException;
use Symfony\Component\Console\Exception\LogicException;

/**
 * Represents a command line argument.
 *
 * @author Fabien Potencier <fabien@symfony.com>
 */
class InputArgument
{
    public const REQUIRED = 1;
    public const OPTIONAL = 2;
    public const IS_ARRAY = 4;

    /**
     * @param string                                                                        $name            The argument name
     * @param int|null                                                                      $mode            The argument mode: a bit mask of self::REQUIRED, self::OPTIONAL and self::IS_ARRAY
     * @param string                                                                        $description     A description text
     * @param string|bool|int|float|array|null                                              $default         The default value (for self::OPTIONAL mode only)
     * @param array|\Closure(CompletionInput,CompletionSuggestions):list<string|Suggestion> $suggestedValues The values used for input completion
     *
     * @throws InvalidArgumentException When argument mode is not valid
     */
    public function __construct(string $name, ?int $mode = null, string $description = '', string|bool|int|float|array|null $default = null, \Closure|array $suggestedValues = [])
    {
    }

    /**
     * Returns the argument name.
     */
    public function getName(): string
    {
    }

    /**
     * Returns true if the argument is required.
     *
     * @return bool true if parameter mode is self::REQUIRED, false otherwise
     */
    public function isRequired(): bool
    {
    }

    /**
     * Returns true if the argument can take multiple values.
     *
     * @return bool true if mode is self::IS_ARRAY, false otherwise
     */
    public function isArray(): bool
    {
    }

    /**
     * Sets the default value.
     *
     * @return void
     *
     * @throws LogicException When incorrect default value is given
     */
    public function setDefault(string|bool|int|float|array|null $default = null)
    {
    }

    /**
     * Returns the default value.
     */
    public function getDefault(): string|bool|int|float|array|null
    {
    }

    public function hasCompletion(): bool
    {
    }

    /**
     * Adds suggestions to $suggestions for the current completion input.
     *
     * @see Command::complete()
     */
    public function complete(CompletionInput $input, CompletionSuggestions $suggestions): void
    {
    }

    /**
     * Returns the description text.
     */
    public function getDescription(): string
    {
    }
}
