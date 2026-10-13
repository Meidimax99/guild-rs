// Headless export: decompiles every function and dumps strings with xrefs.
// Usage: analyzeHeadless <proj> <name> -import <exe> -postScript ExportAll.java <outdir>
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.address.*;
import java.io.*;

public class ExportAll extends GhidraScript {
    @Override
    public void run() throws Exception {
        String out = getScriptArgs()[0];
        DecompInterface di = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        di.setOptions(opts);
        di.openProgram(currentProgram);
        PrintWriter pw = new PrintWriter(new FileWriter(out + "/decomp.c"));
        PrintWriter idx = new PrintWriter(new FileWriter(out + "/functions.tsv"));
        FunctionIterator it = currentProgram.getFunctionManager().getFunctions(true);
        while (it.hasNext() && !monitor.isCancelled()) {
            Function f = it.next();
            idx.println(f.getEntryPoint() + "\t" + f.getName() + "\t" + f.getBody().getNumAddresses());
            DecompileResults r = di.decompileFunction(f, 60, monitor);
            pw.println("// ==== FUNCTION " + f.getEntryPoint() + " " + f.getName());
            if (r != null && r.decompileCompleted()) pw.println(r.getDecompiledFunction().getC());
            else pw.println("// decompile failed");
        }
        pw.close(); idx.close();
        PrintWriter sw = new PrintWriter(new FileWriter(out + "/strings.tsv"));
        DataIterator dit = currentProgram.getListing().getDefinedData(true);
        ReferenceManager rm = currentProgram.getReferenceManager();
        while (dit.hasNext()) {
            Data d = dit.next();
            if (!d.hasStringValue()) continue;
            StringBuilder refs = new StringBuilder();
            for (Reference ref : rm.getReferencesTo(d.getAddress())) {
                Function f = getFunctionContaining(ref.getFromAddress());
                refs.append(ref.getFromAddress()).append("@").append(f == null ? "?" : f.getName()).append(",");
            }
            String v = String.valueOf(d.getValue()).replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t");
            sw.println(d.getAddress() + "\t" + v + "\t" + refs);
        }
        sw.close();
    }
}
