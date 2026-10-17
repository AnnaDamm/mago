<?php

final class NullableCoalesceStoredCallableFailure extends Exception
{
}

final class NullableCoalesceStoredCallable
{
    /** @throws NullableCoalesceStoredCallableFailure */
    public static function run(): void
    {
        throw new NullableCoalesceStoredCallableFailure();
    }
}

function nullable_coalesce_does_not_keep_overwritten_callable(?Closure $maybe): void
{
    $callback = NullableCoalesceStoredCallable::run(...);
    $maybe ?? null;
    $callback = null;
}

function nullable_coalesce_does_not_keep_unset_callable(?Closure $maybe): void
{
    $callback = NullableCoalesceStoredCallable::run(...);
    $maybe ?? null;
    unset($callback);
}
