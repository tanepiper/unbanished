// Decompile selected code addresses in an already analyzed Ghidra project.
// Usage: analyzeHeadless PROJECT NAME -process PROGRAM -noanalysis -scriptPath tools/ghidra \
//   -postScript ExportAddressBatch.java OUTPUT.txt 0xADDRESS [0xADDRESS ...]
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import java.io.*;

public class ExportAddressBatch extends GhidraScript {
  @Override
  public void run() throws Exception {
    String[] args = getScriptArgs();
    File output = new File(args[0]);
    output.getParentFile().mkdirs();
    DecompInterface decompiler = new DecompInterface();
    decompiler.openProgram(currentProgram);
    try (PrintWriter writer = new PrintWriter(new BufferedWriter(new FileWriter(output)))) {
      for (int i = 1; i < args.length && !monitor.isCancelled(); i++) {
        Address address = toAddr(args[i]);
        Function function = currentProgram.getFunctionManager().getFunctionContaining(address);
        if (function == null) {
          if (currentProgram.getListing().getInstructionAt(address) == null) disassemble(address);
          function = createFunction(address, null);
        }
        writer.println("===== ADDRESS " + address + " FUNCTION "
          + (function == null ? "<none>" : function.getEntryPoint() + " " + function.getName(true)));
        if (function == null) {
          writer.println("// no code function could be created at this explicit callback address");
          continue;
        }
        DecompileResults result = decompiler.decompileFunction(function, 60, monitor);
        if (result.decompileCompleted()) writer.println(result.getDecompiledFunction().getC());
        else writer.println("// decompile failed: " + result.getErrorMessage());
        writer.println();
      }
    }
    decompiler.dispose();
    println("Address batch written to " + output);
  }
}
