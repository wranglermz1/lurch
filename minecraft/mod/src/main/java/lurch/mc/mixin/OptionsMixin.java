package lurch.mc.mixin;

import java.io.File;
import java.util.Arrays;

import lurch.mc.ApexKey;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Mutable;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** lurch's keys in Minecraft's Controls screen, added before the options file is read so their saved keys load. */
@Mixin(Options.class)
public class OptionsMixin {
	@Mutable @Shadow @Final public KeyMapping[] keyMappings;

	@Inject(method = "<init>", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Options;load()V"))
	private void lurch$keys(Minecraft minecraft, File dir, CallbackInfo ci) {
		KeyMapping[] all = Arrays.copyOf(keyMappings, keyMappings.length + ApexKey.ALL.length + 1);
		System.arraycopy(ApexKey.ALL, 0, all, keyMappings.length, ApexKey.ALL.length);
		all[all.length - 1] = ApexKey.OPTIONS;
		keyMappings = all;
	}
}
