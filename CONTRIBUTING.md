# Contributing to Crystal Forge

Thanks for your interest in contributing to Crystal Forge! This project is built to solve real compliance and monitoring problems for NixOS deployments in regulated environments.

## Project Status & Goals

Crystal Forge is currently in active development with working core functionality for system monitoring, flake tracking, and deployment enforcement. The project is maintained by Matt (@usmcamp0811) and welcomes contributions from the community.

### Long-term Sustainability

I want to be upfront about the project's future: while Crystal Forge will always be free and open source for homelab and personal use, I plan to eventually offer paid support and features for organizations, companies, and government entities. This commercial offering will help sustain long-term development while keeping the core project accessible to everyone. All core functionality will remain open source.

## How to Contribute

### Finding Work

- Check the [Issues](https://gitlab.com/crystal-forge/crystal-forge/-/issues) for open tasks
- Look for issues labeled `good first issue` or `help wanted`
- Feel free to propose new features or improvements by opening an issue first

### Development Process

1. **Open an issue** or comment on an existing one to discuss what you'd like to work on
2. **Fork the repository** and create a branch for your work
3. **Implement your changes** with appropriate tests
4. **Submit a merge request** when ready for review
5. **Address feedback** - I'll review and work with you to get it merged

### What I'm Looking For

- Working features that solve real problems
- Tests that prove the feature works (integration tests, database tests, or unit tests as appropriate)
- Documentation explaining what changed and how to use it
- Code that doesn't break existing functionality

## Testing Requirements

Crystal Forge has comprehensive testing across multiple levels. When contributing:

**For Rust changes**:

- Add unit tests in the relevant module using `#[cfg(test)]`
- Run tests with `cargo test` or `nix build`

**For database changes**:

- Add database tests in `packages/cf-test-modules/cf_test/tests/database/`
- Test scenarios in `packages/cf-test-modules/cf_test/scenarios/`
- Run with `nix build .#checks.x86_64-linux.database`

**For component integration**:

- Server tests: `nix build .#checks.x86_64-linux.server`
- Builder tests: `nix build .#checks.x86_64-linux.builder`
- Cache tests: `nix build .#checks.x86_64-linux.s3-cache` or `.#checks.x86_64-linux.attic-cache`
- Full test suite: `nix flake check`

See the [Test Plan](docs/test_plan.md) for detailed testing guidance.

## Project Management & Process

I'm actively working on improving project management and development processes. Right now things are fairly loose:

- No strict deadlines or sprints
- Pull requests reviewed as time allows (usually within a few days)
- Discussion happens in issues and merge requests
- I'm learning as I go, especially around Rust best practices

If you have experience with project management or Rust development practices, I'm happy to learn from you.

## What I'm Still Learning

I'm relatively new to:

- Rust (coming from Python/Julia background)
- Some of the more nuanced aspects of the Nix CLI
- Formal development processes

If you spot something I'm doing wrong or could do better, please say something! This project is a learning experience and I appreciate constructive feedback.

## Documentation

When contributing, please:

- Update relevant documentation in `docs/`
- Add comments explaining non-obvious code
- Update the changelog if your change is user-facing
- Include examples where appropriate

## Code Style

- Follow existing patterns in the codebase
- Use `rustfmt` for Rust code formatting
- Keep functions focused and well-named
- Write tests that clearly document expected behavior

## Communication

- Be respectful and constructive
- Ask questions if something isn't clear
- Share context about why you're making changes
- Don't worry about asking "basic" questions - I'm figuring this out too

## Getting Help

- Open an issue for bugs or questions
- Tag me (@usmcamp0811) in merge requests
- Check existing documentation in the `docs/` folder
- Look at existing tests for examples of patterns

## License

By contributing to Crystal Forge, you agree that your contributions will be licensed under the same license as the project (check LICENSE file for current terms).

---

Thanks again for contributing! Every bit helps, whether it's fixing a typo, adding a test, or implementing a major feature.
