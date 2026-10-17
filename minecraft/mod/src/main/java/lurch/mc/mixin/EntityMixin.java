package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.MoverType;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Vanilla's tick still runs for the player lurch moves, but only lurch places it. */
@Mixin(Entity.class)
public class EntityMixin {
	@Inject(method = "move", at = @At("HEAD"), cancellable = true)
	private void lurch$move(MoverType type, Vec3 delta, CallbackInfo ci) {
		if (Lurch.moves(this)) {
			ci.cancel();
		}
	}
}
