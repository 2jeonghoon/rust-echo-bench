#!/bin/bash

folders=$(find . -maxdepth 1 -type d -regextype posix-extended -regex './[0-9]{4}-[0-9]{2}-[0-9]{2}_[0-9]{2}-[0-9]{2}-[0-9]{2}' | sort)


counter=1

for folder in $folders; do
	new_name="lt-$counter"

	# 앞에 ./ 제거
	orig_name="${folder#./}"

	# 이름이 이미 lt-N이면 스킵
	if [[ "$orig_name" == lt-* ]]; then
		continue
	fi
	echo "Renaming '$orig_name' -> '$new_name'"
	mv "$orig_name" "$new_name"
	counter=$((counter + 1))
done
							    




