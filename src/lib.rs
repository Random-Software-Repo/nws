#![allow(unused)]

pub mod nws
{
	use log::*;
	use std::{io,io::Write,env,fs,process,path::Path};
	use serde_json::{Value};
	use chrono::{DateTime,Local,TimeDelta};

	fn get_var(key:&str) -> String
	{
		let empty = String::from("");
		let val = match std::env::var(key)
					{
						Err(e) => {error!("\"{}\"", e);empty},
						Ok(val) => val,
					};
		return val;
	}

	fn get_config_dir() -> String
	{
		// Will return $HOME/.config/nwscache and create the sub-directories of
		// ".config/nwscache" if they don't exist. Otherwise will return "/dev/null"

		let dev_null = String::from("/dev/null");
		let home = get_var("HOME").to_owned();
		if home == ""
		{
			return dev_null;
		}
		let config_base_dir=format!("{}/.config",home);
		let config_base_dir_path = Path::new(config_base_dir.as_str());
		if !config_base_dir_path.exists()
		{
			match fs::create_dir(config_base_dir_path)
			{
				Ok(o)=> o,
				Err(e)=>{error!("Error creating \"{}\":{}",config_base_dir,e);return dev_null},
			};
		}

		let config_dir = format!("{}/nwscache",config_base_dir);
		let config_dir_path = Path::new(config_dir.as_str());
		if !config_dir_path.exists()
		{
			match fs::create_dir(config_dir_path)
			{
				Ok(o)=> o,
				Err(e)=>{error!("Error creating \"{}\":{}",config_dir,e);return dev_null},
			};
		}
		return config_dir
	}

	fn file_valid(file_path:&Path)->bool
	{
		let now = Local::now();
		let name = match file_path.file_name()
			{
				None=>"",
				Some(s)=>match s.to_str(){None=>"",Some(st)=>st,},
			};
		let file_name = String::from(name);
		// convert file_name to a date
		let file_date = match DateTime::parse_from_rfc2822(file_name.as_str())
						{
							Ok(f)=>f,
							Err(_e)=>{return false},
						};
		//match SystemTime::now().duration_since(file_date) 
		let delta = now.signed_duration_since(file_date);
		/*****/
		// if duration is more than 0 seconds, 
		// the file_date is in the past and the 
		// file has expired, and should be deleted
		if delta > TimeDelta::seconds(0)
		{
			// The files are saved with the expiration date as the file name
			// If the current time is more than 0 seconds later than the file_date time
			// the cache has expired and should be deleted.
			match fs::remove_file(file_path)
					{
						Err(e)=>println!("Error deleting expired file \"{}\":{}", file_path.display(),e),
						Ok(o)=>o,
					};
			return false;
		}
		return true
	}


	pub fn purge_config()
	{
		// use println! not logging as this will be called before logging is setup.
		let config_dir = get_config_dir();
		let config_dir_path = Path::new(config_dir.as_str());
		if config_dir_path.exists()
		{
			let entries = match fs::read_dir(config_dir_path)
			{
				Err(_e)=> return,
				Ok(entries)=> entries,
			};
			for possible_entry in entries
			{
				let entry = match possible_entry
					{
						Err(_e)=>continue,
						Ok(entry)=>entry,
					};
				if entry.path().is_dir()
				{
					let mut entry_count = 0;
					let mut deletion_count = 0;
					let dir_entries = match fs::read_dir(entry.path())
					{
						Err(_e)=> continue,
						Ok(dir_entries)=> dir_entries,
					};
					for possible_dir_entry in dir_entries
					{
						entry_count = entry_count +1;
						let file = match possible_dir_entry
							{
								Err(_e)=>continue,
								Ok(file)=>file,
							};
						if file.path().is_file()
						{
							if !file_valid(&file.path())
							{
								deletion_count = deletion_count +1;
							}
						}
					}
					if (entry_count == 0) || (entry_count == deletion_count)
					{
						// entry_count == 0 : the directory had no files in it
						// entry_count = deletion_count : every file in the directory was deleted.
						// in these cases, remove the directory.
						match fs::remove_dir(entry.path())
								{
									Err(e)=>println!("Error deleting empty directory \"{}\":{}", entry.path().display(),e),
									Ok(o)=>o,
								};
					}
				}
			}
		}
	}

	fn cache_response(url:&str, expires:&str, body:&str)->bool
	{
		// the cache responses will be saved in:
		//			$HOME/.config/weathr/<URL>/<EXPIRATION DATE>

		let config_dir = get_config_dir();
		let config_dir_path = Path::new(config_dir.as_str());
		if !config_dir_path.exists()
		{
			match fs::create_dir(config_dir_path)
			{
				Ok(o)=> o,
				Err(e)=>{error!("Error creating \"{}\":{}",config_dir_path.display(),e);return false},
			};
		}
		// the only character in the URLs that is not file-system safe (at least for unix-ish
		// file systems) is the '/'. we'll replace that with th unicode visually similar 
		// character '╱' so that we can simply use the URL as the directory in which to cache
		// the response. This make it easy to find here, and when browsing those directories.
		let fs_safe_url = url.replace("/","╱");
		let cache_dir = format!("{}/{}",config_dir,fs_safe_url);
		let cache_dir_path = Path::new(cache_dir.as_str());
		if !cache_dir_path.exists()
		{
			match fs::create_dir(cache_dir_path)
			{
				Ok(o)=> o,
				Err(e)=>{error!("Error creating \"{}\":{}",cache_dir_path.display(),e);return false},
			};
		}
		// ok, by this point we've created $HOME/.config/weathr/URL
		// now we write body into a file with the expiration as the file name
		if expires != ""
		{
			let file_name = format!("{}/{}",cache_dir, expires);
			match fs::write(file_name.as_str(), body)
			{
				Ok(o)=>o,
				Err(e)=>{error!("Error writing \"{}\":{}",file_name.as_str(),e);return false},
			}
		}
		return true;
	}

	fn get_cached_response(url:&str) -> String
	{
		// Inspects the cached responses, and if one matches the url specified
		// and has not expried, it will be read and returned.
		// If and cached files matching the url specified exist but have expired,
		// they will be deleted.

		let mut data = String::from("");
		let config_dir = get_config_dir();
		let config_dir_path = Path::new(config_dir.as_str());
		if config_dir_path.exists()
		{
			let fs_safe_url = url.replace("/","╱");
			let cache_dir = format!("{}/{}",config_dir,fs_safe_url);
			let cache_dir_path = Path::new(cache_dir.as_str());
			if cache_dir_path.exists()
			{
				// iterate over files in this directory.
				// if any exist with a file name in the past, expired, delete them
				// if any file exists with a file name of now or in the future, read it and return that data.
				let paths = match fs::read_dir(cache_dir_path)
							{
								Ok(paths)=>paths,
								Err(e)=>{error!("Error reading \"{}\":{}",cache_dir_path.display(),e);return data},
							};
				for path in paths
				{
					let p1 = match path
							{
								Ok(p1)=>p1,
								Err(e)=>{error!("Error reading path {}",e); return data},
							};
					if file_valid(&p1.path())
					{
						// file is valid, read data and return that.
						data= match fs::read_to_string(p1.path())
									{
										Err(e)=>{error!("Error reading cached file \"{}\":{}",p1.path().display(),e);data},
										Ok(data)=>{debug!("Returning Cached Data.");data},
									};
					}
					//debug!("****    Name: \"{}\"",file_name);
				}
			}
			else
			{
				debug!("Cache dir \"{}\" doesn't exist.", cache_dir_path.display());
			}
		}
		else
		{
			debug!("Config dir \"{}\" doesn't exist.", config_dir_path.display());
		}
		return data;
	}

	fn print_status(s:&str)
	{
		print!("{}\r", s);
		match io::stdout().flush()
		{
			Ok(_) => return,
			Err(_) => return,
		}
	}

	fn print_erase_line()
	{
		// prints ansi "erase line"
		//               \e [ 2 K
		let word:u32 = 0x1B5B324B;
		let bytes = word.to_be_bytes();
		match io::stdout().write_all(&bytes)
		{
			Ok(_) => return,
			Err(_) => return,
		}
	}

	pub fn call_nws_api(request_url:&str) -> String
	{
		debug!("call_nws_aps \"{}\"", request_url);
		let cached = get_cached_response(request_url);
		if cached != ""
		{
			debug!("Call cached, using that data rather than requesting over http/s.");
			return cached;
		}
		else
		{
			debug!("Call Not Cached!");
			let status_message = format!("Calling \"{}\"", request_url);
			print_status(status_message.as_str());
			let o = match minreq::get(request_url)
					// nws api requires a user-agent header. doesn't matter what. anything will do, but is required.
					.with_header("User-Agent", "weathr-app")
					.send()
					{
						Err(e)=>{error!("Error making nws call:{}",e);process::exit(10)},
						Ok(o)=>o,
					};
			let expires = match o.headers.get("expires")
						{
							Some(expires)=>expires,
							None=> "", // should set to something like now+30 days ???
						};
			debug!("**** Expires : \"{}\"",expires);
			let s = match o.as_str()
				{
					Err(e)=>{error!("Error converting output to String: {}",e);process::exit(11)},
					Ok(s)=>s,
				};
			trace!("call_nws_output:\"{}\"",s);
			print_erase_line();
			cache_response(request_url, expires, s);
			return String::from(s)
		}
	}

	pub fn get_location(pjson: &serde_json::Value, key: &str)->String
	{
		debug!("get_location \"{}\"",key);

		let rl = get_object(pjson, "relativeLocation");
		if *rl != Value::Null
		{
				debug!("got relativeLocation:\n{}", rl);
				let p2 = get_object(rl, "properties");
				if *p2 != Value::Null
				{
					debug!("got second properties:\n{}", p2);
					return get_key(p2, key)
				}
				else
				{
					debug!("failed to get second properties.");
				}
		}
		else
		{
			debug!("failed to get relativeLocation.");
		}
		return String::from("")
	}

	pub fn get_city(pjson: &serde_json::Value)->String
	{
		return get_location(pjson, "city")
	}

	pub fn get_state(pjson: &serde_json::Value)->String
	{
		return get_location(pjson, "state")
	}

	pub fn load_forecast(url:&str) -> serde_json::Value
	{
		let forecast = call_nws_api(url);
		let forecast_json: serde_json::Value = match serde_json::from_str(forecast.as_str())
		{
			Ok(json)=> json,
			Err(e)=>{error!("Error parsing forecast json:{}", e);process::exit(1)},
		};
		return forecast_json;
	}

	pub fn get_key(json:&serde_json::Value, key:&str) -> String
	{
		let empty = String::from("");
		if *json != Value::Null
		{
			if json[key].is_string()
			{
				let val = match json[key].as_str()
						{
							None=> {error!("Error getting \"{}\" from properties as a string.",key);return empty},
							Some(f)=>String::from(f),
						};
				return val;
			}
			else if json[key].is_number()
			{
				let val = match json[key].as_number()
						{
							None=> {error!("Error getting \"{}\" from properties as a number.",key);return empty},
							Some(f)=>f,
						};
				return format!("{}",val);
			}
		}
		return empty
	}

	pub fn get_object<'a,'b>(json:&'a serde_json::Value, key:&'b str) -> &'a serde_json::Value
	{
		if *json != Value::Null
		{
			return &json[key];
		}
		return &Value::Null
	}
	pub fn get_indexed_object<'a,'b>(json:&'a serde_json::Value,key:&'b str,index:usize) -> &'a serde_json::Value
	{
		if *json != Value::Null
		{
			return &json[key][index];
		}
		return &Value::Null
	}

	pub fn get_features_properties(prop:&serde_json::Value,index:usize) -> &serde_json::Value
	{
		let features = get_indexed_object(prop,"features",index);
		if *features != Value::Null
		{
			//return &features["properties"];
			return get_object(features,"properties");
		}
		return &Value::Null
	}

	pub fn get_properties_value_key(json:&serde_json::Value, sub:&str, key:&str) -> String
	{
		let empty = String::from("");
		let pprop = get_object(json,"properties");
		if *pprop != Value::Null
		{
			return get_key(get_object(pprop,sub),key)
		}
		else
		{
			debug!("get_properties_value pprop is null");
		}
		return empty;
	}

	pub fn get_properties_key(json:&serde_json::Value, key:&str) -> String
	{
		let empty = String::from("");
		let pprop = get_object(json,"properties");
		if *pprop != Value::Null
		{
			return get_key(pprop,key)
		}
		else
		{
			debug!("get_properties_value pprop is null");
		}
		return empty;
	}

	pub fn get_features_properties_key(prop:&serde_json::Value,index:usize, key:&str) -> String
	{
		let empty = String::from("");
		let properties = get_features_properties(prop,index);
		if *properties != Value::Null
		{
			return get_key(properties,key);
		}
		return empty
	}

	pub fn get_features_key(prop:&serde_json::Value,index:usize, key:&str) -> String
	{
		let empty = String::from("");
		//"features" "0" "id"

		//let features:&Value = &prop["features"];

		let s1 = get_indexed_object(prop,"features",index);
		if *s1 != Value::Null
		{
			return get_key(s1,key)
		}
		return empty;
	}

	pub fn get_features_properties_value<'a, 'b>(prop:&'a serde_json::Value, index:usize, value:&'b str) -> &'a serde_json::Value
	{
		let fproperties = get_features_properties(prop, index);
		if *fproperties != Value::Null
		{
			return &fproperties[value];
		}
		return &Value::Null
	}

	pub fn get_features_properties_value_key(prop:&serde_json::Value, index:usize, value:&str, key:&str) -> String
	{
		let empty = String::from("");
		let vprop = get_features_properties_value(prop,index,value);
		if *vprop != Value::Null
		{
			return get_key(vprop,key)
		}
		else
		{
			debug!("get_features_properties_value_key vprop \"{}\" is null",value);
		}
		return empty;
	}

	pub fn get_points_url(latlong:&str)->String
	{
		let url:String;
		if latlong == ""
		{
			url = String::from("");
		}
		else
		{
			url = format!("https://api.weather.gov/points/{}",latlong);
		}
		return url
	}

}