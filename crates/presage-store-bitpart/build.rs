// presage-store-bitpart
// Copyright (C) 2025 Throneless Tech
//
// This code is derived in part from code from the Presage project:
// Copyright (C) 2024 Gabriel Féron

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::io::Result;
use std::path::Path;

fn main() -> Result<()> {
    let protobuf = Path::new("src/protobuf").to_owned();

    // Build script does not automagically rerun when a new protobuf file is added.
    // Directories are checked against mtime, which is platform specific
    println!("cargo:rerun-if-changed=src/protobuf");

    let input: Vec<_> = protobuf
        .read_dir()
        .expect("protobuf directory")
        .filter_map(|entry| {
            let entry = entry.expect("readable protobuf directory");
            let path = entry.path();
            if Some("proto") == path.extension().and_then(std::ffi::OsStr::to_str) {
                assert!(path.is_file());
                println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
                Some(path)
            } else {
                None
            }
        })
        .collect();

    prost_build::compile_protos(&input, &[protobuf])?;

    Ok(())
}
