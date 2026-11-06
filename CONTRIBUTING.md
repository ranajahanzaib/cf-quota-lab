## Contributing

We welcome contributions of all kinds to improve this project! Here's how you can get involved:

1. **Fork the Repository**: Start by forking the repository to your GitHub account.

2. **Clone Your Fork**: Clone the forked repository to your local machine:

   ```bash
   git clone <repo_url>
   cd repo_dir
   ```

3. **Create a Branch**: Create a new branch for your feature or bug fix:

   ```bash
   git checkout -b feature-name
   ```

4. **Make Changes**: Make your changes to the codebase. Be sure to follow existing code style and conventions.

5. **Commit Changes**: Commit your changes with a clear and descriptive message:

   ```bash
   git commit -m "Add detailed description of the changes"
   ```

6. **Push Changes**: Push your changes to your fork:

   ```bash
   git push origin feature-name
   ```

7. **Submit a Pull Request**: Open a pull request from your branch to the main repository's branch. Include a detailed description of your changes and the problem they solve.

8. **Review and Feedback**: Participate in the review process. Make any requested changes, and your contribution will be merged!

### Guidelines

- Ensure your code follows the existing style and conventions.
- Include tests for any new functionality or bug fixes.
- Write clear, concise commit messages and pull request descriptions.

### Issues

Feel free to report bugs or suggest enhancements by creating an issue. Be sure to include detailed information about the problem and, if possible, steps to reproduce it.

### QuotaLab development

Install with `corepack pnpm install --frozen-lockfile` and `cargo build --workspace`. Before submitting, run
`cargo fmt --all -- --check`, warning-denied Clippy, `cargo test --workspace`,
`pnpm build`, `pnpm typecheck`, `pnpm test`, `pnpm test:integration`,
`pnpm check:schemas`, and `pnpm format:check`.

Keep metric provenance explicit. Missing resource measurements must remain unknown;
new wrappers need tests for native receivers, return values, exceptions and
concurrent request isolation. Changes to configuration/report models require
regenerating schemas using `target/debug/quotalab schemas`. Do not include private
request data, bound SQL parameters or test tokens in fixtures or report artifacts.
