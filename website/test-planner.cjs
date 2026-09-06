'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const cp=require('node:child_process');
const {quote,buildTrial}=require('./assets/site.js');
const base={version:'0.1.13',command:'MarkDuplicates',input:'/data/sample.bam',jar:'/opt/picard/picard.jar',reference:''};
let checks=0;
function check(fn){fn();checks++;}
check(()=>assert.equal(buildTrial(base),fs.readFileSync(__dirname+'/default-trial.sh','utf8').trim()));
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
check(()=>assert.throws(()=>buildTrial({...base,version:'0.1.13; touch bad'})));
check(()=>assert.throws(()=>buildTrial({...base,jar:'https://example.com/picard.jar'})));
check(()=>assert.equal(quote("a'b"),"'a'\"'\"'b'"));
console.log(JSON.stringify({planner_checks:checks,status:'PASS',commands_executed:false}));
