package lurch.mc;

import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Properties;

import net.minecraft.client.Minecraft;

/**
 * The player's lurch options and HUD layout, kept in `config/lurch.properties`. An option the
 * player never set is not kept: lurch-mc's own (its flags and `lurch.cfg`) stands until they do.
 */
public final class LurchConfig {
	/** One HUD element's place: its top left corner moved by (dx, dy), as fractions of the screen, drawn `scale` times its size. */
	public record Place(float dx, float dy, float scale) {
		public static final Place NONE = new Place(0, 0, 1);

		public boolean moved() {
			return !equals(NONE);
		}
	}

	private static final Properties props = new Properties();
	private static boolean loaded;

	private static Path file() {
		return Minecraft.getInstance().gameDirectory.toPath().resolve("config").resolve("lurch.properties");
	}

	private static Properties props() {
		if (!loaded) {
			loaded = true;
			try (Reader r = Files.newBufferedReader(file())) {
				props.load(r);
			} catch (IOException ignored) {
			}
		}
		return props;
	}

	public static void save() {
		try {
			Files.createDirectories(file().getParent());
			try (Writer w = Files.newBufferedWriter(file())) {
				props.store(w, "lurch options and HUD layout");
			}
		} catch (IOException e) {
			System.err.println("lurch: saving " + file() + ": " + e);
		}
	}

	/** The option the player set, or null. */
	public static String option(String name) {
		return props().getProperty("option." + name);
	}

	public static void setOption(String name, String value) {
		props().setProperty("option." + name, value);
		save();
	}

	/** Every option the player set, as lurch-mc's `set` lines. */
	public static String setLines() {
		StringBuilder sb = new StringBuilder();
		for (String k : props().stringPropertyNames()) {
			if (k.startsWith("option.")) {
				sb.append("set ").append(k.substring(7)).append(' ').append(props.getProperty(k)).append('\n');
			}
		}
		return sb.toString();
	}

	public static Place place(String element) {
		String v = props().getProperty("layout." + element);
		if (v == null) {
			return Place.NONE;
		}
		try {
			String[] p = v.split(",");
			return new Place(Float.parseFloat(p[0]), Float.parseFloat(p[1]), Float.parseFloat(p[2]));
		} catch (RuntimeException e) {
			return Place.NONE;
		}
	}

	/** Sets a piece's place in memory; `save` writes it. */
	public static void setPlace(String element, Place p) {
		if (p.moved()) {
			props().setProperty("layout." + element, p.dx() + "," + p.dy() + "," + p.scale());
		} else {
			props().remove("layout." + element);
		}
	}

	public static void resetLayout() {
		props().stringPropertyNames().stream().filter(k -> k.startsWith("layout.")).forEach(props::remove);
		save();
	}
}
