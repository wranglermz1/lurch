import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import java.io.*;
public class Dec extends GhidraScript {
  public void run() throws Exception {
    String[] a = getScriptArgs();
    DecompInterface d = new DecompInterface();
    d.openProgram(currentProgram);
    PrintWriter out = new PrintWriter(new FileWriter(a[0]));
    for (int i = 1; i < a.length; i++) {
      Address ad = toAddr(Long.decode(a[i]));
      disassemble(ad);
      Function f = getFunctionAt(ad);
      if (f == null) f = createFunction(ad, null);
      if (f == null) { out.println("// no function at " + a[i]); continue; }
      DecompileResults r = d.decompileFunction(f, 120, monitor);
      out.println("// ==== " + a[i]);
      out.println(r.getDecompiledFunction() == null ? r.getErrorMessage() : r.getDecompiledFunction().getC());
    }
    out.close();
  }
}
