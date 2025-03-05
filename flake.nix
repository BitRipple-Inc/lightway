{
  description = "LightWay protocl reference implementation";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
      };
    in {
      packages = {
        default = pkgs.rustPlatform.buildRustPackage rec {
          name = "lightway";
          # version = 0.1.0;
	  src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "wolfssl-3.0.0" = "sha256-v1EnXrSJFUAEIF38oHzNW6Y3Z/vejMZfyq26PBP6YGU=";
            };
          };
          checkFlags = [
            # Some tests fail with "nix build", but not when running
            # "cargo test" directly.  For at least some of those, it
            # appears to be that they use networking.
            "--skip=wire::data::tests::maximum_packet_size_for_plpmtu::_0_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data::tests::maximum_packet_size_for_plpmtu::_1_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data::tests::maximum_packet_size_for_plpmtu::_2_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data::tests::maximum_packet_size_for_plpmtu::_3_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x00001_false_expects_panicking_some_fragment_offset_must_be_8_byte_aligned_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x00002_false_expects_panicking_some_fragment_offset_must_be_8_byte_aligned_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x00003_false_expects_panicking_some_fragment_offset_must_be_8_byte_aligned_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x00004_false_expects_panicking_some_fragment_offset_must_be_8_byte_aligned_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x00005_false_expects_panicking_some_fragment_offset_must_be_8_byte_aligned_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x00006_false_expects_panicking_some_fragment_offset_must_be_8_byte_aligned_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x00007_false_expects_panicking_some_fragment_offset_must_be_8_byte_aligned_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x0ffff_false_expects_panicking_some_fragment_offset_must_be_8_byte_aligned_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x10000_false_expects_panicking_some_offset_must_fit_in_16_bits_"
            "--skip=wire::data_frag::tests::encode_offset_and_mf::_0x1000f_false_expects_panicking_some_offset_must_fit_in_16_bits_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_0_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_10_expects_panicking_some_chunk_size_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_11_expects_panicking_some_chunk_size_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_12_expects_panicking_some_chunk_size_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_13_expects_panicking_some_chunk_size_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_14_expects_panicking_some_chunk_size_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_1_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_2_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_3_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_4_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_5_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_6_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_7_expects_panicking_some_plpmtu_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_8_expects_panicking_some_chunk_size_too_small_"
            "--skip=wire::data_frag::tests::maximum_packet_size_for_plpmtu::_9_expects_panicking_some_chunk_size_too_small_"
          ];
          nativeBuildInputs = with pkgs; [
            autoconf
            automake
            libtool
            clang

            apacheHttpd  # for htpasswd
          ];
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
	};
      };
    });
}

# vim:sw=2:sts=2:et
