ls -lah *
git clone https://github.com/unbounce/iidy.git iidy-js
#cd iidy-js
#npm install


GH_TOKEN=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
OWNER=tavisrudd
REPO=gptplay
TAG=test

# Step 1: Get release ID by tag
release_id=$(curl -s -H "Authorization: Bearer $GH_TOKEN" \
  https://api.github.com/repos/$OWNER/$REPO/releases/tags/$TAG | jq -r .id)

for ASSET_NAME in cargo-cache.zip cargo-target.zip; do
# Step 2: Find the asset ID
asset_id=$(curl -s -H "Authorization: Bearer $GH_TOKEN" \
  https://api.github.com/repos/$OWNER/$REPO/releases/$release_id/assets \
  | jq -r ".[] | select(.name == \"$ASSET_NAME\") | .id")

# Step 3: Download the asset
if [[ -n "$asset_id" ]]; then
  curl -L \
    -H "Authorization: Bearer $GH_TOKEN" \
    -H "Accept: application/octet-stream" \
    https://api.github.com/repos/$OWNER/$REPO/releases/assets/$asset_id \
    -o $ASSET_NAME
else
  echo "Asset '$ASSET_NAME' not found in release '$TAG'."
fi

done
unzip -q cargo-target.zip -d iidy/target
rm cargo-target.zip
ls -lah *
unzip -q cargo-cache.zip -d ~/.cargo
ls -lah ~/.cargo
rm cargo-cache.zip || true
cd iidy
rustup component add rustfmt
cargo fetch 
#tar --warning=no-unknown-keyword -xzf rust-deps-cache.tar.gz -C . || true
#rm rust-deps-cache.tar.gz
#chmod +x iidy-linux-x64; mv iidy-linux-x64 ../iidy-js-linux-x64-bin
ls -lah *
echo setup done


# ls -lah *
# git clone https://github.com/unbounce/iidy.git iidy-js
# cd iidy-js
# #npm install

# cd ../iidy
# rustup component add rustfmt
# cargo fetch 
# GH_TOKEN=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
# OWNER=tavisrudd
# REPO=gptplay
# TAG=test
# ASSET_NAME=rust-deps-cache.tar.gz

# # Step 1: Get release ID by tag
# release_id=$(curl -s -H "Authorization: Bearer $GH_TOKEN" \
#   https://api.github.com/repos/$OWNER/$REPO/releases/tags/$TAG | jq -r .id)

# for ASSET_NAME in rust-deps-cache.tar.gz iidy-linux-x64; do
# # Step 2: Find the asset ID
# asset_id=$(curl -s -H "Authorization: Bearer $GH_TOKEN" \
#   https://api.github.com/repos/$OWNER/$REPO/releases/$release_id/assets \
#   | jq -r ".[] | select(.name == \"$ASSET_NAME\") | .id")

# # Step 3: Download the asset
# if [[ -n "$asset_id" ]]; then
#   curl -L \
#     -H "Authorization: Bearer $GH_TOKEN" \
#     -H "Accept: application/octet-stream" \
#     https://api.github.com/repos/$OWNER/$REPO/releases/assets/$asset_id \
#     -o $ASSET_NAME
# else
#   echo "Asset '$ASSET_NAME' not found in release '$TAG'."
# fi

# done

# tar --warning=no-unknown-keyword -xzf rust-deps-cache.tar.gz -C . || true
# rm rust-deps-cache.tar.gz
# chmod +x iidy-linux-x64; mv iidy-linux-x64 ../iidy-js-linux-x64-bin
# ls -lah *
# echo setup done


## unpack registry cache
#/usr/bin/tar -xf /home/runner/work/_temp/7b37f4b9-caba-4638-ac00-263761e4c6d0/cache.tzst -P -C /home/runner/work/gptplay/gptplay --use-compress-program unzstd


## unpack build cache
#/usr/bin/tar -xf /home/runner/work/_temp/cbcc65f7-14ca-4bb4-acee-306ae189ed58/cache.tzst -P -C /home/runner/work/gptplay/gptplay --use-compress-program unzstd


## pack caches
#/usr/bin/tar --posix -cf cache.tzst --exclude cache.tzst -P -C /home/runner/work/gptplay/gptplay --files-from manifest.txt --use-compress-program zstdmt
#/usr/bin/tar --posix -cf cache.tzst --exclude cache.tzst -P -C /home/runner/work/gptplay/gptplay --files-from manifest.txt --use-compress-program zstdmt
