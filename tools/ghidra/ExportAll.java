import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.data.*;
import java.io.*;
public class ExportAll extends GhidraScript {
  public void run() throws Exception {
    String outDir = getScriptArgs()[0];
    String name = currentProgram.getName();
    File d = new File(outDir, name); d.mkdirs();
    try (PrintWriter w = new PrintWriter(new File(d, "imports.txt"))) {
      for (Symbol s : currentProgram.getSymbolTable().getExternalSymbols())
        w.println(s.getParentNamespace().getName() + "!" + s.getName());
    }
    try (PrintWriter w = new PrintWriter(new File(d, "exports.txt"))) {
      SymbolIterator it = currentProgram.getSymbolTable().getAllSymbols(true);
      while (it.hasNext()) { Symbol s = it.next(); if (s.isExternalEntryPoint()) w.println(s.getAddress() + "\t" + s.getName(true)); }
    }
    try (PrintWriter w = new PrintWriter(new File(d, "strings.txt"))) {
      DataIterator it = currentProgram.getListing().getDefinedData(true);
      while (it.hasNext()) { Data x = it.next(); if (x.hasStringValue()) w.println(x.getAddress() + "\t" + String.valueOf(x.getValue()).replace("\n","\\n")); }
    }
    DecompInterface di = new DecompInterface(); di.openProgram(currentProgram);
    try (PrintWriter f = new PrintWriter(new File(d, "functions.tsv")); PrintWriter c = new PrintWriter(new File(d, "decomp.c"))) {
      for (Function fn : currentProgram.getFunctionManager().getFunctions(true)) {
        if (monitor.isCancelled()) break;
        f.println(fn.getEntryPoint() + "\t" + fn.getBody().getNumAddresses() + "\t" + fn.getName(true) + "\t" + (fn.isThunk()?"thunk":""));
        if (fn.isThunk() || fn.isExternal()) continue;
        DecompileResults r = di.decompileFunction(fn, 60, monitor);
        c.println("// ==== " + fn.getEntryPoint() + " " + fn.getName(true));
        c.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// decompile failed: " + r.getErrorMessage());
      }
    }
    println("EXPORT DONE " + name);
  }
}
