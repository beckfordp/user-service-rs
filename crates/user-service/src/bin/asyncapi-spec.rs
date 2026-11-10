// SPDX-License-Identifier: MIT

//! Prints the service's AsyncAPI document (compact JSON, as purekafka writes
//! `target/asyncapi-docs-spec/asyncapi.json`), the input
//! `scripts/asyncapi-html.sh` renders to HTML.

fn main() {
    println!("{}", user_service::events::asyncapi());
}
