'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const cp=require('node:child_process');
const os=require('node:os');
const {quote,buildTrial}=require('./assets/site.js');
const base={version:'0.1.14',command:'MarkDuplicates',input:'/data/sample.bam',jar:'/opt/picard/picard.jar',reference:''};
let checks=0;
function check(fn){fn();checks++;}
check(()=>assert.equal(
 buildTrial(base),
 fs.readFileSync(__dirname+'/default-trial.sh','utf8').trim().replace(/\{\{VERSION\}\}/g,base.version)
));
for (const command of ['MarkDuplicates','SortSam','SamToFastq','CollectMultipleMetrics']) {
 check(()=>assert.match(buildTrial({...base,command}),new RegExp('--commands '+command)));
}
for (const input of ['/data/a b.bam',"/data/O'Brien.bam",'/data/$(touch PWNED).bam','/data/`echo bad`.bam','/data/"double".bam']) {
 check(()=>{
  const script=buildTrial({...base,input,jar:"/opt/O'Brien/$() picard.jar"});
  cp.execFileSync('bash',['-n'],{input:script}); // Parse only. Never execute.
  const parser='import json,shlex,sys; a=shlex.split(sys.stdin.read().replace("\\\\\\n"," ")); i=a.index("--input-bam"); j=a.index("--picard-command"); print(json.dumps([a[i+1], shlex.split(a[j+1])]))';
  const trial=script.slice(script.indexOf('"$work/venv/bin/python" "$work/source/tools/compare_real_data.py"'));
  const parsed=JSON.parse(cp.execFileSync('python3',['-c',parser],{input:trial,encoding:'utf8'}));
  assert.equal(parsed[0],input);assert.deepEqual(parsed[1],['java','-jar',"/opt/O'Brien/$() picard.jar"]);
 });
}
for (const input of ['relative.bam','/data/a\nb.bam','/data/a\0.bam','/data/a.sam',''])
 check(()=>assert.throws(()=>buildTrial({...base,input})));
check(()=>assert.throws(()=>buildTrial({...base,input:'/data/a.cram'}),/Reference/));
check(()=>assert.match(buildTrial({...base,input:'/data/a.cram',reference:'/ref/a.fa'}),/--reference-fasta '\/ref\/a.fa'/));
check(()=>assert.throws(()=>buildTrial({...base,command:'CollectRnaSeqMetrics'})));
check(()=>assert.throws(()=>buildTrial({...base,version:base.version+'; touch bad'})));
check(()=>assert.throws(()=>buildTrial({...base,jar:'https://example.com/picard.jar'})));
check(()=>assert.equal(quote("a'b"),"'a'\"'\"'b'"));
// Recovery must identify the failing control without changing accepted paths.
for (const [changes,fieldId,code] of [
 [{input:''},'trial-input','input.required'],
 [{input:'relative.bam'},'trial-input','input.relative'],
 [{input:'/'+'a'.repeat(4092)+'.bam'},'trial-input','input.length'],
 [{input:'/data/a\u007f.bam'},'trial-input','input.controls'],
 [{input:'/data/a.sam'},'trial-input','input.extension'],
 [{jar:''},'trial-jar','jar.required'],
 [{jar:'picard.jar'},'trial-jar','jar.relative'],
 [{jar:'/bin/turbo-picard'},'trial-jar','jar.extension'],
 [{jar:'/a\tb.jar'},'trial-jar','jar.controls'],
 [{input:'/data/a.cram'},'trial-reference','reference.required'],
 [{reference:'refs/a.fa'},'trial-reference','reference.relative'],
 [{reference:'/refs/a\nb.fa'},'trial-reference','reference.controls'],
 [{reference:'/'+'a'.repeat(4096)},'trial-reference','reference.length'],
 [{command:'Unknown'},'trial-command','command.unsupported'],
 [{version:'not-a-version'},null,'version.invalid'],
]) check(()=>assert.throws(()=>buildTrial({...base,...changes}),e=>e.fieldId===fieldId && e.code===code && e.message.length<=180));
check(()=>assert.doesNotThrow(()=>buildTrial({...base,input:'/'+'a'.repeat(4091)+'.bam'})));
check(()=>assert.doesNotThrow(()=>buildTrial({...base,input:'/data/研究 🧬.BAM',reference:'   '})));
const bash=cp.execFileSync('bash',['-c','printf %s "$BASH"'],{encoding:'utf8'});
// Resolve through pyenv/env wrappers before restricting PATH to the prerequisite sandbox.
const python=cp.execFileSync('python3',['-c','import os,sys; print(os.path.realpath(sys.executable))'],{encoding:'utf8'}).trim();
for (const [label,script] of [
 ['generated',buildTrial(base)],
 ['static',fs.readFileSync(__dirname+'/default-trial.sh','utf8').replace(/\{\{VERSION\}\}/g,base.version)],
]) {
 const bin=fs.mkdtempSync(os.tmpdir()+'/turbo-picard-prereqs-');
 try {
  fs.symlinkSync(python,bin+'/python3');
  for (const tool of ['git','java']) fs.writeFileSync(bin+'/'+tool,'#!/bin/sh\nexit 97\n',{mode:0o755});
  fs.writeFileSync(bin+'/mktemp','#!/bin/sh\nexit 37\n',{mode:0o755});
  const run=()=>cp.spawnSync(bash,['-c',script],{env:{...process.env,PATH:bin},encoding:'utf8'});
  check(()=>assert.equal(run().status,37,label+' must reach workspace creation without samtools'));
  for (const tool of ['python3','git','java']) {
   fs.renameSync(bin+'/'+tool,bin+'/'+tool+'.disabled');
   check(()=>{
    const result=run();
    assert.equal(result.error,undefined);
    assert.notEqual(result.status,0,label+' must reject missing '+tool);
    assert.notEqual(result.status,37,label+' must reject missing '+tool+' before creating a workspace');
   });
   fs.renameSync(bin+'/'+tool+'.disabled',bin+'/'+tool);
  }
 } finally {fs.rmSync(bin,{recursive:true,force:true});}
}
console.log(JSON.stringify({planner_checks:checks,status:'PASS',workload_commands_executed:false}));
