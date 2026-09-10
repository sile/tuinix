//! Helpers shared by the property-based test binaries.

/// Runs a property with a time-based seed, overridable via the
/// `TUINIX_PBT_SEED` environment variable for deterministic reproduction of a
/// reported failure.
///
/// The runner is returned so that coverage-gate assertions can embed its seed
/// in the failure message.
pub fn run<F>(cases: usize, f: F) -> noprop::TestResult<noprop::Runner>
where
    F: Fn(&mut noprop::TestCaseContext) -> noprop::TestResult,
{
    let seed = noprop::seed_from_env_or_time("TUINIX_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(cases, f)?;
    Ok(runner)
}
