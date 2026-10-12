#!/usr/bin/env python3
"""CI-only adapter. All request data is derived from verified source/provider context.
No workflow input accepts arbitrary command text, tool paths or a JSON request.
Provider artifact authentication remains an independent consumer obligation.
"""
import datetime, hashlib, json, os, pathlib, re, shutil, subprocess, sys, tomllib
P = pathlib.Path
LIMIT = 512 * 1024 * 1024

def fail(message):
    raise ValueError(message)

def read(path, limit=LIMIT):
    path = P(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size > limit:
        fail(f"nonregular/oversize input: {path}")
    data = path.read_bytes()
    if len(data) > limit: fail("input grew beyond budget")
    return data

def digest(data): return hashlib.sha256(data).hexdigest()
def canonical(value): return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
def write(path, value):
    with P(path).open("xb") as f: f.write(canonical(value) + b"\n")
def command(args, cwd=None, timeout=30):
    result = subprocess.run(args, cwd=cwd, check=True, capture_output=True, timeout=timeout)
    if len(result.stdout)+len(result.stderr) > 1024*1024: fail("setup output exceeds budget")
    return result.stdout.decode().strip()
def git(*args): return command(["git", "--no-replace-objects", "-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false", *args])
def oid(value): return bool(re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", value)) and set(value)!={"0"}
def checked_attempt(raw):
    if not re.fullmatch(r"[1-9][0-9]{0,9}",raw) or int(raw)>4294967295: fail("invalid canonical workflow run attempt")
    return raw

def source_context():
    if os.environ.get("BUILDKITE_BUILD_ID"): fail("GitHub-owned packaging cannot run on a Buildkite importer")
    required = ["SOURCE_HEAD","ACCEPTED_BASE","TRANSPORT_HEAD","SOURCE_TREE","SNAPSHOT_REQUEST","SOURCE_REF"]
    context = {key: os.environ[key] for key in required}
    h,b,s,t = (context[k] for k in required[:4])
    if not all(map(oid,[h,b,s,t])): fail("immutable source context is incomplete")
    request = context["SNAPSHOT_REQUEST"]
    if not re.fullmatch(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}",request) or set(request.replace('-',''))=={'0'}: fail("invalid snapshot request")
    if os.environ.get("GITHUB_EVENT_NAME") != "push" or os.environ.get("GITHUB_REF") != "refs/heads/decapod/preflight/"+request or os.environ.get("GITHUB_SHA")!=s:
        fail("release proof is restricted to verified snapshot pushes")
    message = f"decapod CI preflight snapshot\n\nschema=1\nrequest={request}\nsource={h}\nbase={b}\nsource_ref={context['SOURCE_REF']}\n"
    if git("show","-s","--format=%B",s)+"\n" != message: fail("snapshot metadata differs from verified context")
    if git("show","-s","--format=%P",s)!=h or git("rev-parse",s+"^{tree}")!=t or git("rev-parse",h+"^{tree}")!=t or git("rev-parse","HEAD")!=h:
        fail("snapshot/source parent or tree mismatch")
    if git("rev-parse","refs/remotes/origin/master")!=b: fail("accepted target changed")
    git("merge-base","--is-ancestor",b,h)
    timestamp=int(git("show","-s","--format=%ct",s))
    context["release_date"] = datetime.datetime.fromtimestamp(timestamp,datetime.timezone.utc).date().isoformat()
    attempt=checked_attempt(os.environ["GITHUB_RUN_ATTEMPT"])
    context.update(run_id=os.environ["GITHUB_RUN_ID"],run_attempt=attempt,job=os.environ["GITHUB_JOB"],
                   runner=os.environ["PROOF_RUNNER"],runner_os=os.environ["RUNNER_OS"],runner_arch=os.environ["RUNNER_ARCH"])
    return context

def assert_release_matrix(configuration, workflow, plan=None):
    # Deferred runners are supported by the pinned importer only without a
    # matrix. Every finite local job declares the actual production target and
    # runner, with an unused supported fallback for inapplicable provider jobs.
    if re.search(r"^  local:\n",workflow,re.M): fail('dynamic or matrix local job is unsupported')
    jobs=re.findall(r"(?ms)^  (local_[a-z0-9_]+):\n(.*?)(?=^  [a-z][a-z0-9_-]*:\n|\Z)",workflow)
    if not jobs: fail('missing explicit local release jobs')
    declared=[]
    fallback=configuration['dist']['github-custom-runners']['global']
    for job,body in jobs:
        if '    strategy:' in body or '      matrix:' in body: fail('local release jobs must not use matrices')
        metadata=re.findall(r"^    env:\n      PROOF_TARGET: ([a-z0-9_-]+)\n      PROOF_RUNNER: ([a-z0-9.-]+)\n",body,re.M)
        if len(metadata)!=1: fail('explicit release job metadata is missing or ambiguous')
        target,runner=metadata[0]
        expected="    runs-on: ${{ needs.prepare.outputs.applicable == 'true' && '"+runner+"' || '"+fallback+"' }}"
        if body.count('    runs-on:')!=1 or expected not in body.splitlines(): fail('release runner selection differs from declared production applicability')
        if "    if: needs.prepare.outputs.applicable == 'true'" not in body.splitlines(): fail('release provider applicability guard missing')
        if f'    name: local ({target}, {runner})' not in body.splitlines(): fail('release job display identity differs from metadata')
        declared.append((target,runner))
    dist=configuration['dist'];targets=dist['targets'];runners=dist['github-custom-runners']
    if len(set(targets))!=len(targets) or len({t for t,_ in declared})!=len(declared): fail('duplicate release target')
    expected=sorted((t,runners[t]) for t in targets)
    if sorted(declared)!=expected: fail('declared matrix differs from production dist target/runner configuration')
    if plan is not None:
        rows=plan['ci']['github']['artifacts_matrix']['include'];actual=[]
        if not isinstance(rows,list) or not rows: fail('actual release plan matrix missing')
        for entry in rows:
            if not isinstance(entry.get('targets'),list) or not entry['targets'] or not isinstance(entry.get('runner'),str): fail('actual release plan row invalid')
            actual.extend((t,entry['runner']) for t in entry['targets'])
        if sorted(actual)!=expected: fail('actual release plan matrix differs from declared production matrix')
    return {'include':[{'target':target,'runner':runner} for target,runner in declared]}

def verify_current_matrix(plan=None):
    return assert_release_matrix(tomllib.loads(read('dist-workspace.toml').decode()),read('.github/workflows/preflight-release.yml').decode(),plan)

def setup(context):
    root=P.cwd().resolve()
    pins=tomllib.loads(read(root/".github/release-tools.toml",65536).decode())
    definition = P(os.environ['RUNNER_WORKSPACE']).parent/'_actions'/pins['semver_checks_installer'].replace('@','/')/'action.yml'
    installer=read(definition,1024*1024)
    sources = ['.github/release-tools.toml','.github/release.toml','dist-workspace.toml','dist.toml',
        '.github/workflows/release.yml','.github/actions/setup-release-proof/action.yml',
        '.github/scripts/release-tool-versions.py','.github/scripts/install-release-dist.sh']
    configuration={name:digest(read(root/name)) for name in sources}
    contract=digest(canonical({'source_files':configuration,'installer_definition_sha256':digest(installer)}))
    paths={name:str(P(shutil.which(exe) or fail('missing '+exe)).resolve()) for name,exe in [
        ('git','git'),('release_plz','release-plz'),('cargo_dist','cargo-dist'),('cargo_semver_checks','cargo-semver-checks'),('curl','curl'),('xz','xz')]}
    for name in ['cargo','rustc']: paths[name]=str(P(command(['rustup','which',name])).resolve())
    versions={}
    for name in ['git','cargo','rustc','release_plz','cargo_dist','cargo_semver_checks']:
        argv=[paths[name],*(['dist'] if name=='cargo_dist' else []),'--version']
        output=command(argv)
        fields=output.split()
        version=fields[2] if name=='git' else fields[1]
        versions[name]={'version':version,'binary_sha256':digest(read(paths[name])),'argv':argv,'stdout':output}
    for key,pin in [('cargo','rust'),('rustc','rust'),('release_plz','release_plz'),('cargo_dist','cargo_dist')]:
        if versions[key]['version']!=pins[pin]: fail('resolved version differs from shared pin: '+key)
    if not re.fullmatch(re.escape(pins['semver_checks_constraint'])+r'\.\d+',versions['cargo_semver_checks']['version']): fail('unresolved semver-checks constraint')
    measured={key:{'actual_version':row['version'],'executable_sha256':row['binary_sha256'],
        'production_versions':{'shared-reviewed-setup:'+context['runner']:row['version']},
        'production_resolution_sha256':contract} for key,row in versions.items() if key!='cargo_semver_checks'}
    build={}
    if context['runner_os']=='macOS':
        build['SDKROOT']=command(['xcrun','--sdk','macosx','--show-sdk-path'])
        build['DEVELOPER_DIR']=command(['xcode-select','-p'])
    if os.environ.get('PROOF_TARGET')=='aarch64-unknown-linux-gnu':
        build['CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER']=str(P(shutil.which('aarch64-linux-gnu-gcc') or fail('missing reviewed cross linker')).resolve())
    request={'source_root':str(root),'source_head':context['SOURCE_HEAD'],'accepted_base':context['ACCEPTED_BASE'],
        'release_date':context['release_date'],'tools':paths,'production_tools':measured,
        'semver_checks_production_version':versions['cargo_semver_checks']['version'],
        'semver_checks_installer_definition_sha256':digest(installer),'build_environment':build}
    receipt={'schema':'derived-release-shared-setup/1','contract_sha256':contract,'source_files':configuration,
        'installer_definition_sha256':digest(installer),'installer':pins['semver_checks_installer'],
        'runner':context['runner'],'runner_os':context['runner_os'],'runner_arch':context['runner_arch'],
        'tools':versions,'build_environment':build,'authority':'same reviewed production setup; measured here, no historical production binary claim'}
    return request,receipt,installer

def run(role, output, transport=None):
    output=P(output); output.mkdir()
    context=source_context(); verify_current_matrix(); request,receipt,installer=setup(context)
    write(output/'context.json',context);write(output/'setup.json',receipt)
    (output/'setup-inputs').mkdir()
    with (output/'setup-inputs/semver-checks-installer-action.yml').open('xb') as f: f.write(installer)
    cli=P('target/debug/decapod-derived-release-proof').resolve()
    if not cli.is_file(): fail('reviewed source-built proof driver missing')
    stages=['prepare'] if role=='prepare' else ['plan','package','global','candidate'] if role=='foundation' else ['local'] if role=='local' else fail('invalid release stage')
    if role!='prepare':
        transport=P(transport)
        if digest(read(transport/'transport.json',1024*1024))!=os.environ['EXPECTED_TRANSPORT_SHA256'] or digest(read(transport/'derived.bundle'))!=os.environ['EXPECTED_BUNDLE_SHA256']:
            fail('downloaded derivative differs from prepare job outputs')
    reports=[]
    for stage in stages:
        owned=output/stage;owned.mkdir()
        write(owned/'request.json',request)
        args=[str(cli),'--request',str((owned/'request.json').resolve()),'--scratch',str(owned.resolve()),'--stage',stage]
        if transport: args+=['--transport-root',str(transport.resolve())]
        if stage=='local': args+=['--target',os.environ['PROOF_TARGET']]
        # Driver bounds every command/process tree/output internally. This outer
        # guard bounds the entire stage and stores actual driver diagnostics.
        with (owned/'driver.stdout').open('xb') as out,(owned/'driver.stderr').open('xb') as err:
            completed=subprocess.run(args,stdout=out,stderr=err,timeout=3900)
        if completed.returncode: fail(f'proof stage failed: {stage} ({completed.returncode})')
        reports.append({'stage':stage,'report_sha256':digest(read(owned/'derived-release-proof.json')),
            'driver_sha256':digest(read(cli)),'driver_argv':args,'exit_code':completed.returncode,
            'stdout_sha256':digest(read(owned/'driver.stdout')),'stderr_sha256':digest(read(owned/'driver.stderr'))})
    write(output/'execution.json',{'schema':'derived-release-ci/1','context':context,'setup':receipt,'stages':reports})
    if role=='prepare':
        transport=output/'prepare/transport'
        matrix=verify_current_matrix(json.loads(read(transport/'evidence/plan-dist-manifest.json',16*1024*1024)))
        write(output/'verified-release-matrix.json',matrix)
        with open(os.environ['GITHUB_OUTPUT'],'a') as f:
            f.write('transport_sha256='+digest(read(transport/'transport.json'))+'\n')
            f.write('bundle_sha256='+digest(read(transport/'derived.bundle'))+'\n')
            f.write('release_date='+context['release_date']+'\n')

def aggregate(directory):
    context=source_context();verify_current_matrix();_,receipt,_=setup(context)
    environment=os.environ.copy()
    environment.update(RELEASE_DATE=context['release_date'],SETUP_CONTRACT_SHA256=receipt['contract_sha256'])
    subprocess.run([str(P('target/debug/decapod-derived-release-proof').resolve()),'--aggregate',directory],check=True,env=environment,timeout=300)

if __name__=='__main__':
    try:
        if len(sys.argv)==3 and sys.argv[1]=='aggregate': aggregate(sys.argv[2])
        elif len(sys.argv) in [4,5] and sys.argv[1]=='run': run(*sys.argv[2:])
        else: fail('usage: preflight-release.py run ROLE OWNED_OUTPUT [TRANSPORT] | aggregate DIRECTORY')
    except (ValueError,KeyError,OSError,subprocess.SubprocessError) as e:
        print('Release proof failed: '+str(e),file=sys.stderr);sys.exit(1)
