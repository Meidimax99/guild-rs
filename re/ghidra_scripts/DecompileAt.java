// Create functions (if missing) at the given addresses and decompile them.
// Args: <outfile> <addr>...   Run with -process against an analyzed project.
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import java.io.*;

public class DecompileAt extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        PrintWriter pw = new PrintWriter(new FileWriter(args[0]));
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        for (int i = 1; i < args.length; i++) {
            Address a = toAddr(args[i]);
            Function f = getFunctionAt(a);
            if (f == null) {
                new DisassembleCommand(a, null, true).applyTo(currentProgram, monitor);
                new CreateFunctionCmd(a).applyTo(currentProgram, monitor);
                f = getFunctionAt(a);
            }
            pw.println("// ==== FUNCTION " + a + " " + (f == null ? "?" : f.getName()));
            if (f == null) { pw.println("// could not create"); continue; }
            DecompileResults r = di.decompileFunction(f, 60, monitor);
            pw.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// failed");
        }
        pw.close();
    }
}
