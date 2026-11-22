// Find code that auto-analysis did not turn into functions and create them.
//
// Walks the executable code blocks. Every address that is neither inside a
// function nor defined data nor alignment padding is a candidate; candidates
// that follow padding, a RET/JMP or the end of a function (or are 16-byte
// aligned) are disassembled and made into functions. Auto-analysis runs on
// the new code after each pass so that their callees are found too. Repeats
// until a pass finds nothing. Prints one summary line per pass.
//
// Run: analyzeHeadless <dir> proj -process GildeGold_TL.exe -noanalysis \
//        -scriptPath re/ghidra_scripts -postScript FindFunctions.java
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;

public class FindFunctions extends GhidraScript {
    private boolean isPad(byte b) {
        return b == (byte) 0x90 || b == (byte) 0xcc || b == 0;
    }

    /// The byte before `a` ends something: padding, RET, JMP, INT3.
    private boolean boundaryBefore(Address a) throws Exception {
        Address p = a.previous();
        if (p == null) return true;
        Function f = getFunctionContaining(p);
        if (f != null && !f.getBody().contains(a)) return true;
        Instruction i = getInstructionContaining(p);
        if (i != null) {
            String m = i.getMnemonicString();
            return m.startsWith("RET") || m.equals("JMP") || m.equals("INT3");
        }
        return isPad(getByte(p));
    }

    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        int total = 0;
        for (int pass = 1; pass <= 12 && !monitor.isCancelled(); pass++) {
            int created = 0, failed = 0;
            for (MemoryBlock b : mem.getBlocks()) {
                if (!b.isExecute() || !b.isInitialized()) continue;
                Address a = b.getStart();
                Address end = b.getEnd();
                while (a != null && a.compareTo(end) <= 0 && !monitor.isCancelled()) {
                    Function f = getFunctionContaining(a);
                    if (f != null) {
                        AddressRange r = f.getBody().getRangeContaining(a);
                        a = r.getMaxAddress().next();
                        continue;
                    }
                    Data d = getDataContaining(a);
                    if (d != null && d.isDefined()) {
                        a = d.getMaxAddress().next();
                        continue;
                    }
                    if (isPad(getByte(a))) {
                        a = a.next();
                        continue;
                    }
                    boolean aligned = (a.getOffset() & 0xf) == 0;
                    if (aligned || boundaryBefore(a)) {
                        if (getInstructionAt(a) == null) {
                            new DisassembleCommand(a, null, true).applyTo(currentProgram, monitor);
                        }
                        if (getInstructionAt(a) != null && new CreateFunctionCmd(a).applyTo(currentProgram, monitor)) {
                            Function nf = getFunctionAt(a);
                            if (nf != null) {
                                created++;
                                a = nf.getBody().getRangeContaining(a).getMaxAddress().next();
                                continue;
                            }
                        }
                        failed++;
                    }
                    a = a.next();
                }
            }
            analyzeChanges(currentProgram);
            total += created;
            println("FindFunctions pass " + pass + ": created " + created + ", rejected " + failed
                    + ", functions now " + currentProgram.getFunctionManager().getFunctionCount());
            if (created == 0) break;
        }
        println("FindFunctions total created: " + total);
    }
}
