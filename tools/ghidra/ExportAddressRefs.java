// Export direct references and their containing functions for selected Ghidra addresses.
// Usage: analyzeHeadless PROJECT NAME -process PROGRAM -noanalysis -scriptPath tools/ghidra \
//   -postScript ExportAddressRefs.java OUTPUT.txt 0xADDRESS [0xADDRESS ...]
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;

public class ExportAddressRefs extends GhidraScript {
  @Override
  public void run() throws Exception {
    String[] args = getScriptArgs();
    File output = new File(args[0]);
    if (output.getParentFile() != null) output.getParentFile().mkdirs();
    DecompInterface decompiler = new DecompInterface();
    decompiler.openProgram(currentProgram);
    try (PrintWriter writer = new PrintWriter(new BufferedWriter(new FileWriter(output)))) {
      FunctionManager functions = currentProgram.getFunctionManager();
      ReferenceManager references = currentProgram.getReferenceManager();
      for (int i = 1; i < args.length && !monitor.isCancelled(); i++) {
        Address target = toAddr(args[i]);
        Function function = functions.getFunctionAt(target);
        writer.println("===== TARGET " + target + " "
          + (function == null ? "<no function>" : function.getName(true)));
        ReferenceIterator refs = references.getReferencesTo(target);
        while (refs.hasNext() && !monitor.isCancelled()) {
          Reference ref = refs.next();
          Function caller = functions.getFunctionContaining(ref.getFromAddress());
          writer.println("REF " + ref.getReferenceType() + " from=" + ref.getFromAddress()
            + " function=" + (caller == null ? "<none>"
              : caller.getEntryPoint() + " " + caller.getName(true)));
          if (caller != null) {
            DecompileResults result = decompiler.decompileFunction(caller, 60, monitor);
            if (result.decompileCompleted()) writer.println(result.getDecompiledFunction().getC());
            else writer.println("// decompile failed: " + result.getErrorMessage());
          }
        }
        writer.println();
      }
    } finally {
      decompiler.dispose();
    }
    println("Address reference export written to " + output);
  }
}
