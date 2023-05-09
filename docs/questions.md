### Reading questions for chapters of the 'Computer Architecture - A Quantitative Approach' book

### Appendix A
```
1. When taking instruction-caches into account, how does the real-world performance difference
actually look when considering optimizations focused on speed vs optimizations on code
size. I.e. can code-size optimizations sometimes outperform other types of optimizations.

2. With compilers iteratively performing different optimizations, which optimizations end up being
the most relevant, and do some optimizations actually end up frequently having negative effects?
```

### 1.1-1.3
```
1. Given that PMD's have hard requirements on how much power they can consume due to not having
access to proper cooling, is there a hard limit on the processor-performance for such devices
that we are currently approaching?

2. Why is decoding instructions made so complicated? Eg. when taking RISC-V J-Type instructions, why
is the immediate split up into 4 different sections that the decoder needs to piece together. Does
that not slow down the decoding process?
```

### 1.4-1.7
```
1. The chapter mentions testing of wafers and dies. How is this testing performed, and what are
some factors that could lead to these having errors during manufacture.

2. Every now and then, high impact cpu bugs are discovered that threaten the security or
dependability of cpu's. How are these bugs usually found, and how can manufacturers improve on
their testing methods to more reliably find these themselves before shipping.
```

### 1.8-1.13
```
1. The book mentions multiple issues with various performance specs but continues to use
them. Nowadays there are projects like https://google.github.io/fuzzbench/ available that span
thousands of fairly well distributed open source projects. Would something like that not be better?

2. These chapters talk a good bit about fine grained performance tracking (how much execution
time is spent in common instructions, etc). How are these numbers usually obtained, and how
would a cpu-research environment usually look. Is it generally just based on Simulators like Bochs?
```

### B.1-B.3
```
1. The book briefly mentions that larger caches result in longer hit-times alongside higher
   power/energy consumption. The latter makes sense, but why would hit-times be longer? Assuming the
   same speed cache is used (obviously this gets a lot more expensive with a larger cache) and the
   associativity isn't increased, a cache-search just indexes an array. Why does a larger sized
   cache potentially slow down this access?

2. Why does the first-level cache affect the clock-rate of the processor while the second level does
   not?
```

### 2.1-2.3
```
1. Why are write buffers used? Does that not duplicate memory from the cache & the write buffer?
   Would it not be easier if a list of cache-sets to be written back was simply maintained?

2. How do non-blocking caches provide such large benefits? Let's assume an L1-miss-penalty of 10.
   For this optimization to be helpful, would it not be required that the processor not operate on
   the requested data for at least 10 cycles? If it does it would have to stall anyways until the
   data finally comes in. With	modern processors frequently executing 2-4 instructions per cycle,
   this seems like an unlikely edge case rather than the norm.
```

### C
```
1. Most exceptions don't seem strict enough to a point where they would have to be immediately
   executed. Would it not often be reasonable in many cases to finish up the remaining instructions
   in the pipeline before flushing it and handling the exception? (I know that some exceptions like
   eg. page-faults do not have that luxury, but mainly curious about the general case that might
   benefit from this).

2. The chapter describes that how if an exception occurs, for precise exception handling, the next
   few instructions in the pipeline need to be finished to verify that this was the first
   instruction that threw an exception. Is this part of the reason for spectre, where a protection
   fault is thrown from an invalid memory access, and then further instructions are executed while
   the pipeline is emptied?
```

### 3.1-3.5
```
1. Since flushing the pipeline after a branch-misprediction is such a big issue, were there ever
   attempts to just execute both possible paths? To do this the pipeline could be backed by a
   shadowed pipeline that is filled alongside the original. At a branch, both paths would be
   executed. Once the comparison result comes in, the correct pipeline is maintained and the
   incorrect one is discarded. If not, is building pipeline-designs in hardware just too expensive?

2. Page 199 mentions: "To preserve exception behavior, no instruction is allowed to initiate
   execution until a branch that precedes the instruction in program order has been completed." With
   that in mind, I am a little confused how spectre became a thing. The exploits seem to generally
   be reliant on loading a section of memory into the L1-cache, which would occur during the memory
   stage of the pipeline. In all designs we have looked at thus far, this stage occured after the
   execution stage. If both of the above are true then it would be impossible for memory to be
   loaded into the cache while speculatively executing branch code since the memory pipeline-stage
   would be stalled until after the branch-result is known. What am I missing here?
```

### 3.6-3.10
```
1. On an architecture that uses a larger set of physical registers, how exactly are correct values
   displayed during debugging? It seems like at any time, a lot of instructions will have values in
   various renamed physical registers. When an interrupt occurs, is this entire mapping completely
   synced up to the official-isa registers that a user would expect before being discarded, or does
   the cpu have additional translation layers that is in charge of giving the user the information
   they expect to see while the real layer is in reality entirely different.

2. The chapter mentions a technique called branch folding. How exactly does that work? The
   target-address still needs to be calculated, so even with out simple pipeline the instruction
   would still need to go through fetch, decode, and execute stages to compute the branch target. Is
   this more of an optimization for future hits of the instructions after the first time where the
   exec-result is cached so only the fetch/decode need to be performed?
```

### 3.11-3.15
```
1. What are some good resources to further read up on the performance properties of hyperthreading? 
   In my own testing (8-core; 2-threads each), I was only able to achieve about a 20% increase in 
   performance when running a process (no syscalls, pure cpu/memory) with 8 vs 16 kernel-threads. 
   This was a good bit lower than I would have expected given the complexity it requires.

2. From the description of the reading it seems like adding hyperthreading to a processor has a
   better power-efficiency/speedup ratio than adding additional cores. Wouldn't this encourage
   mobile devices such as smartphones to include hyperthreading over adding more cores? It does not
   seem like they make use of this technology yet.
```

### B.4-B.5
```
1. Having the cpu traverse page tables on every non-tlb memory access is expensive. Assuming
   complete removal of page tables, how much of a speed up could we achieve? With that in mind,
   would specialized hardware that removes the page-table overhead be worth it for special use-cases
   that require high performance and don't care about security to a point where they want to be the
   only application running on the system anyways?

2. C isn't going anywhere anytime soon, so memory corruption security will be an issue for a long
   time. With that in mind, could more fine grained or even tagged memory be a thing? Basically a
   mechanism during which the application can request memory with very specific access permissions,
   and the cpu being able to enforce them. Something much more fine-grained than the current
   page-level permission bits.
```

### 2.4-2.9
```
1. Hypervisors provide interesting methods of additional security without having to modify the
   underlying architecture. On Windows for example, extended page tables are now used to verify that
   no writable kernel memory ever becomes executable. Linux in comparison has been going backwards
   on this, adding JIT compilers to their kernels so that something like this is entirely
   impossible. What other virtualization based protections have been recently developed, and are any
   of them opt-out default (unlike SGX) so that they are actually in-use on modern desktops?

2. It seems like ept mostly replaced shadow page tables due to performance reasons. Where does the
   main overhead come in here? With ept-traversals, up to 24-memory accesses need to be performed to
   translate a non-tlb address, while shadow page table walks seem to be linear.
```

### D.1-D.4
### 5.1-5.3
### 5.4
### 5.5-5.11
### 4.1-4.3
### 4.4-4.10
### 6.1-6.4
### 6.5-6.10
