/*
* SPDX-FileCopyrightText: 2026 Nextcloud GmbH and Nextcloud contributors
* SPDX-License-Identifier: AGPL-3.0-or-later
*/
final: prev: let
  inherit (prev) lib;
in {
  nextcloud-notify_push = prev.nextcloud-notify_push.overrideAttrs (finalAttrs: previousAttrs: {
    version = "develop";
    src = lib.sources.sourceByRegex (lib.cleanSource ../.) [
      "Cargo.*"
      "(src|tests|test_client|build.rs|appinfo)(/.*)?"
    ];

    cargoDeps = prev.rustPlatform.importCargoLock {lockFile = ../Cargo.lock;};
  });
}
