// Apply names from a TSV (address<TAB>name[<TAB>comment]) to the current program:
// functions are renamed (created if missing), other addresses get a primary label.
// Usage: analyzeHeadless <proj> <name> -process <prog> -postScript ApplyNames.java names.tsv
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;

public class ApplyNames extends GhidraScript {
    @Override
    public void run() throws Exception {
        BufferedReader br = new BufferedReader(new FileReader(getScriptArgs()[0]));
        String line;
        int n = 0;
        while ((line = br.readLine()) != null) {
            line = line.trim();
            if (line.isEmpty() || line.startsWith("#")) continue;
            String[] p = line.split("\t");
            if (p.length < 2) continue;
            Address a = toAddr(p[0]);
            String name = p[1];
            boolean isCode = currentProgram.getMemory().getBlock(a) != null
                && currentProgram.getMemory().getBlock(a).isExecute();
            if (isCode) {
                Function f = getFunctionAt(a);
                if (f == null) {
                    new DisassembleCommand(a, null, true).applyTo(currentProgram, monitor);
                    new CreateFunctionCmd(a).applyTo(currentProgram, monitor);
                    f = getFunctionAt(a);
                }
                if (f != null) f.setName(name, SourceType.USER_DEFINED);
                else createLabel(a, name, true, SourceType.USER_DEFINED);
            } else {
                createLabel(a, name, true, SourceType.USER_DEFINED);
            }
            if (p.length >= 3 && !p[2].isEmpty()) setPlateComment(a, p[2]);
            n++;
        }
        br.close();
        println("applied " + n + " names");
    }
}
