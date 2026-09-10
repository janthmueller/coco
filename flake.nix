{
  description = "CoCo — local control plane for Codex workspaces";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        packageVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

        cocoPackage = rustPlatform.buildRustPackage {
          pname = "codex-coordinator";
          version = packageVersion;
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.lock
              ./Cargo.toml
              ./LICENSE
              ./README.md
              ./src
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          doCheck = false;

          meta = {
            description = "Local control plane for persistent Codex workspaces";
            homepage = "https://github.com/janthmueller/coco";
            license = pkgs.lib.licenses.mit;
            mainProgram = "coco";
            platforms = pkgs.lib.platforms.linux ++ pkgs.lib.platforms.darwin;
          };
        };

        app =
          drv: description:
          (flake-utils.lib.mkApp { inherit drv; })
          // {
            meta = { inherit description; };
          };

        binaryApp =
          binary: description:
          (flake-utils.lib.mkApp {
            drv = cocoPackage;
            exePath = "/bin/${binary}";
          })
          // {
            meta = { inherit description; };
          };

        requireWorkspace = ''
          if [ ! -f Cargo.toml ]; then
            echo "Run this app from the CoCo repository root." >&2
            exit 1
          fi
        '';

        cargoRuntimeInputs = [
          rustToolchain
          pkgs.cargo-deny
          pkgs.cargo-machete
          pkgs.git
          pkgs.procps
          pkgs.stdenv.cc
          pkgs.pkg-config
        ];

        docsRuntimeInputs = [
          pkgs.nodejs_24
          pkgs.pnpm
        ];

        cargoCommand =
          name: script:
          pkgs.writeShellApplication {
            name = "coco-${name}";
            runtimeInputs = cargoRuntimeInputs;
            text = ''
              ${requireWorkspace}
              ${script}
            '';
          };

        docsCommand =
          name: script:
          pkgs.writeShellApplication {
            name = "coco-docs-${name}";
            runtimeInputs = docsRuntimeInputs;
            text = ''
              ${requireWorkspace}
              if [ ! -f docs/package.json ]; then
                echo "The CoCo documentation project is missing." >&2
                exit 1
              fi
              ${script}
            '';
          };

        build = cargoCommand "build" ''
          exec cargo build --locked "$@"
        '';

        test = cargoCommand "test" ''
          exec cargo test --locked "$@"
        '';

        check = cargoCommand "check" ''
          exec cargo check --locked --all-targets --all-features "$@"
        '';

        fmt = cargoCommand "fmt" ''
          exec cargo fmt --all -- --check
        '';

        clippy = cargoCommand "clippy" ''
          exec cargo clippy --locked --all-targets --all-features -- -D warnings
        '';

        deps = cargoCommand "deps" ''
          exec cargo machete "$@"
        '';

        policy = cargoCommand "policy" ''
          exec cargo deny check "$@"
        '';

        package = cargoCommand "package" ''
          exec cargo publish --locked --dry-run "$@"
        '';

        run-coco = cargoCommand "run-coco" ''
          exec cargo run --locked --bin coco -- "$@"
        '';

        run-cocod = cargoCommand "run-cocod" ''
          exec cargo run --locked --bin cocod -- "$@"
        '';

        mcp = cargoCommand "mcp" ''
          exec cargo run --locked --bin coco-mcp -- "$@"
        '';

        docs-install = docsCommand "install" ''
          exec pnpm --dir docs install --frozen-lockfile "$@"
        '';

        docs-dev = docsCommand "dev" ''
          exec pnpm --dir docs run dev -- "$@"
        '';

        docs-check = docsCommand "check" ''
          exec pnpm --dir docs run check -- "$@"
        '';

        docs-build = docsCommand "build" ''
          exec pnpm --dir docs run build -- "$@"
        '';

        docs-preview = docsCommand "preview" ''
          exec pnpm --dir docs run preview -- "$@"
        '';

        app-check = pkgs.symlinkJoin {
          name = "coco-app-check";
          paths = [
            build
            test
            check
            fmt
            clippy
            deps
            policy
            package
            run-coco
            run-cocod
            mcp
            docs-install
            docs-dev
            docs-check
            docs-build
            docs-preview
          ];
        };

        tooling-check =
          pkgs.runCommand "coco-tooling-check"
            {
              nativeBuildInputs = [
                rustToolchain
                pkgs.actionlint
                pkgs.cargo-deny
                pkgs.cargo-machete
                pkgs.git
                pkgs.jq
                pkgs.nodejs_24
                pkgs.pnpm
                pkgs.pkg-config
                pkgs.python3
                pkgs.sqlite
              ];
              cargoManifest = ./Cargo.toml;
              cargoLock = ./Cargo.lock;
              licenseFile = ./LICENSE;
              publicReadme = ./README.md;
              toolchainManifest = ./rust-toolchain.toml;
              docsManifest = ./docs/package.json;
              workflowDirectory = ./.github/workflows;
              releaseConfig = ./releaserc.toml;
              releaseVersionSync = ./.github/scripts/sync_cargo_lock.py;
              releaseVersionTests = ./.github/scripts/test_sync_cargo_lock.py;
            }
            ''
              metadata_project="$TMPDIR/coco-metadata"
              mkdir -p "$metadata_project/src/bin"
              cp "$cargoManifest" "$metadata_project/Cargo.toml"
              cp "$cargoLock" "$metadata_project/Cargo.lock"
              cp "$licenseFile" "$metadata_project/LICENSE"
              cp "$publicReadme" "$metadata_project/README.md"
              touch "$metadata_project/src/lib.rs"
              touch "$metadata_project/src/bin/coco.rs"
              touch "$metadata_project/src/bin/cocod.rs"
              touch "$metadata_project/src/bin/coco-mcp.rs"

              export CARGO_HOME="$TMPDIR/cargo-home"
              cargo metadata \
                --format-version 1 \
                --locked \
                --manifest-path "$metadata_project/Cargo.toml" \
                --no-deps \
                --offline \
                > "$TMPDIR/metadata.json"

              jq -e '
                (.packages | length) == 1
                and (.packages[0].name == "codex-coordinator")
                and (.packages[0].edition == "2024")
                and (.packages[0].rust_version == "1.98.1")
                and (.packages[0].license == "MIT")
                and (.packages[0].repository == "https://github.com/janthmueller/coco")
                and (.packages[0].homepage == "https://janthmueller.github.io/coco/")
                and (.packages[0].publish == ["crates-io"])
                and any(.packages[0].targets[]; .name == "coco" and .kind == ["lib"])
                and any(.packages[0].targets[]; .name == "coco" and .kind == ["bin"])
                and any(.packages[0].targets[]; .name == "cocod" and .kind == ["bin"])
                and any(.packages[0].targets[]; .name == "coco-mcp" and .kind == ["bin"])
              ' "$TMPDIR/metadata.json" >/dev/null

              jq -e '
                .name == "coco-docs"
                and .private == true
                and .packageManager == "pnpm@11.21.0"
                and .scripts.build == "next build --webpack && node ./scripts/verify-export.mjs"
              ' "$docsManifest" >/dev/null

              grep -F 'channel = "1.98.1"' "$toolchainManifest" >/dev/null
              rustc --version | grep -F 'rustc 1.98.1 ' >/dev/null
              cargo --version >/dev/null
              rustfmt --version >/dev/null
              cargo clippy --version >/dev/null
              cargo deny --version >/dev/null
              cargo machete --version >/dev/null
              actionlint "$workflowDirectory"/*.yml
              COCO_RELEASE_SCRIPT="$releaseVersionSync" python3 "$releaseVersionTests"
              grep -F 'version_toml = ["Cargo.toml:package.version"]' "$releaseConfig" >/dev/null
              grep -F 'assets = ["Cargo.lock"]' "$releaseConfig" >/dev/null
              rust-analyzer --version >/dev/null
              pkg-config --version >/dev/null
              sqlite3 --version >/dev/null
              git --version >/dev/null
              jq --version >/dev/null
              node --version >/dev/null
              pnpm --version >/dev/null

              mkdir -p "$out"
              touch "$out/passed"
            '';
      in
      {
        packages = {
          default = cocoPackage;
          coco = cocoPackage;
        };

        apps = {
          default = binaryApp "coco" "Run the CoCo CLI";
          coco = binaryApp "coco" "Run the CoCo CLI";
          cocod = binaryApp "cocod" "Run the CoCo daemon";
          coco-mcp = binaryApp "coco-mcp" "Run the CoCo MCP stdio server";
          build = app build "Build CoCo";
          test = app test "Run the CoCo test suite";
          check = app check "Check all CoCo Rust targets";
          fmt = app fmt "Check CoCo Rust formatting";
          clippy = app clippy "Lint all CoCo Rust targets";
          deps = app deps "Check CoCo for unused direct Rust dependencies";
          policy = app policy "Check CoCo dependency advisories, licenses, sources, and bans";
          package = app package "Verify the CoCo crates.io package";
          run-coco = app run-coco "Run the CoCo CLI";
          run-cocod = app run-cocod "Run the CoCo daemon";
          mcp = app mcp "Run the CoCo MCP stdio server";
          docs-install = app docs-install "Install pinned documentation dependencies";
          docs-dev = app docs-dev "Run the documentation development server";
          docs-check = app docs-check "Check documentation sources";
          docs-build = app docs-build "Build and verify the static documentation export";
          docs-preview = app docs-preview "Serve the static documentation export locally";
        };

        checks = {
          default = tooling-check;
          tooling = tooling-check;
          apps = app-check;
          package = cocoPackage;
        };

        formatter = pkgs.nixfmt;

        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            pkgs.actionlint
            pkgs.cargo-deny
            pkgs.cargo-machete
            pkgs.git
            pkgs.jq
            pkgs.nixfmt
            pkgs.nodejs_24
            pkgs.pnpm
            pkgs.pkg-config
            pkgs.python3
            pkgs.sqlite
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };
      }
    );
}
