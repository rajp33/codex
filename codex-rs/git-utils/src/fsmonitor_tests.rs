use std::collections::VecDeque;
use std::future::Future;

use pretty_assertions::assert_eq;

use super::FsmonitorOverride;
use super::FsmonitorProbeRunner;
use super::detect_fsmonitor_override;

type ProbeRequest = (Vec<&'static str>, Vec<(&'static str, &'static str)>);

struct FakeRunner {
    outputs: VecDeque<Option<Vec<u8>>>,
    requests: Vec<ProbeRequest>,
}

impl FsmonitorProbeRunner for FakeRunner {
    fn run_probe(
        &mut self,
        args: &'static [&'static str],
        env: &'static [(&'static str, &'static str)],
    ) -> impl Future<Output = Option<Vec<u8>>> + Send {
        self.requests.push((args.to_vec(), env.to_vec()));
        let output = self.outputs.pop_front().expect("missing probe output");
        std::future::ready(output)
    }
}

#[tokio::test]
async fn detects_only_supported_builtin_fsmonitor() {
    let cases = [
        (vec![None], FsmonitorOverride::Disabled, /*calls*/ 1),
        (
            vec![Some(b"/tmp/fsmonitor-helper\0".to_vec())],
            FsmonitorOverride::Disabled,
            /*calls*/ 1,
        ),
        (
            vec![Some(b"true\0".to_vec()), None],
            FsmonitorOverride::Disabled,
            /*calls*/ 2,
        ),
        (
            vec![Some(b"true\0".to_vec()), Some(Vec::new())],
            FsmonitorOverride::Disabled,
            /*calls*/ 2,
        ),
        (
            vec![
                Some(b"true\0".to_vec()),
                Some(b"feature: fsmonitor--daemon\n".to_vec()),
            ],
            FsmonitorOverride::BuiltIn,
            /*calls*/ 2,
        ),
    ];

    for (outputs, expected, calls) in cases {
        let mut runner = FakeRunner {
            outputs: outputs.into(),
            requests: Vec::new(),
        };

        let actual = detect_fsmonitor_override(&mut runner).await;

        let mut expected_requests = vec![(
            vec!["config", "--null", "--get", "core.fsmonitor"],
            vec![("GIT_OPTIONAL_LOCKS", "0"), ("LC_ALL", "C")],
        )];
        if calls == 2 {
            expected_requests.push((
                vec!["version", "--build-options"],
                vec![("GIT_OPTIONAL_LOCKS", "0"), ("LC_ALL", "C")],
            ));
        }
        assert_eq!((actual, runner.requests), (expected, expected_requests));
    }
}
