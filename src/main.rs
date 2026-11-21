use decapod::core::{error::DecapodError, group_broker};

fn main() {
    if let Err(error) = decapod::run() {
        if let DecapodError::DiagnosticExit(code) = error {
            std::process::exit(code);
        }
        eprintln!("{error}");
        let exit_code = match error {
            DecapodError::MutationRejectedBeforeWrite(_)
                if group_broker::is_internal_invocation() =>
            {
                group_broker::PRE_MUTATION_POLICY_DENIAL_EXIT
            }
            _ => 1,
        };
        std::process::exit(exit_code);
    }
}
